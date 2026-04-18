import fs from "fs";
import path from "path";
import http from "http";
import readline from "readline";
import { WebSocketServer } from "ws";

const PORT = Number(process.env.VIZ_PORT || 8080);
const PUBLIC_DIR = path.join(process.cwd(), "public");
const LOG_DIR = process.env.VIZ_LOG_DIR || "";
const POLL_MS = Number(process.env.VIZ_LOG_POLL_MS || 300);

const mimeTypes = {
  ".html": "text/html; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8"
};

const state = {
  regions: new Map(), // regionShort -> {x, y}
  claimsByRoundRegion: new Map() // `${round}:${region}` -> Set(agent)
};

function keyFor(round, region) {
  return `${round}:${region}`;
}

function sendJson(ws, obj) {
  ws.send(JSON.stringify(obj));
}

function isEventShape(event) {
  return event && typeof event === "object" && typeof event.type === "string";
}

function broadcast(wss, event) {
  if (!isEventShape(event)) return;
  for (const client of wss.clients) {
    if (client.readyState === 1) sendJson(client, event);
  }
}

function parseClaims(orderText) {
  return orderText
    .split(">")
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => p.split(":")[0])
    .filter(Boolean);
}

function parseLogLine(agentId, line) {
  if (!line) return [];

  const events = [];
  const claimMatch = line.match(/claim publish region=([0-9a-f]+) x=(\d+) y=(\d+) .* round=(\d+)/);
  if (claimMatch) {
    const region = claimMatch[1];
    const x = Number(claimMatch[2]);
    const y = Number(claimMatch[3]);
    const round = Number(claimMatch[4]);
    state.regions.set(region, { x, y });

    const key = keyFor(round, region);
    const claims = state.claimsByRoundRegion.get(key) || new Set();
    claims.add(agentId);
    state.claimsByRoundRegion.set(key, claims);

    events.push({
      type: "agent_position",
      agent_id: agentId,
      x,
      y
    });
  }

  const winnerMatch = line.match(/winner region=([0-9a-f]+) round=(\d+) order=(.*) winner=([a-zA-Z0-9_-]+)/);
  if (winnerMatch) {
    const region = winnerMatch[1];
    const round = Number(winnerMatch[2]);
    const order = winnerMatch[3];
    const winner = winnerMatch[4];
    const regionPos = state.regions.get(region) || { x: 0, y: 0 };
    const claimsFromOrder = parseClaims(order);
    const knownClaims = Array.from(state.claimsByRoundRegion.get(keyFor(round, region)) || []);
    const mergedClaims = Array.from(new Set([...knownClaims, ...claimsFromOrder]));

    events.push({
      type: "round_update",
      round,
      target: { x: regionPos.x, y: regionPos.y },
      region: { x: regionPos.x, y: regionPos.y },
      claims: mergedClaims,
      winner
    });
  }

  if (line.includes("claim lost, rerouting immediately")) {
    events.push({
      type: "event",
      name: "reroute",
      agent_id: agentId
    });
  }

  const invalidated = line.match(/owner_invalidated\(region_id\) region=([0-9a-f]+)/);
  if (invalidated) {
    const region = invalidated[1];
    const pos = state.regions.get(region) || { x: 0, y: 0 };
    events.push({
      type: "event",
      name: "owner_invalidated",
      region: [pos.x, pos.y]
    });
  }

  return events;
}

function startLogPolling(wss, logDir) {
  const offsets = new Map();

  setInterval(() => {
    let files = [];
    try {
      files = fs.readdirSync(logDir).filter((f) => f.endsWith(".log"));
    } catch {
      return;
    }

    for (const file of files) {
      const fullPath = path.join(logDir, file);
      const agentId = path.basename(file, ".log");
      let text = "";
      const prevOffset = offsets.get(fullPath) || 0;
      try {
        const stat = fs.statSync(fullPath);
        if (stat.size < prevOffset) offsets.set(fullPath, 0);
        const nextOffset = offsets.get(fullPath) || 0;
        if (stat.size === nextOffset) continue;
        const fd = fs.openSync(fullPath, "r");
        const buf = Buffer.alloc(stat.size - nextOffset);
        fs.readSync(fd, buf, 0, buf.length, nextOffset);
        fs.closeSync(fd);
        offsets.set(fullPath, stat.size);
        text = buf.toString("utf8");
      } catch {
        continue;
      }

      for (const line of text.split("\n")) {
        const events = parseLogLine(agentId, line);
        for (const event of events) broadcast(wss, event);
      }
    }
  }, POLL_MS);
}

function serveStatic(req, res) {
  let reqPath = req.url || "/";
  if (reqPath === "/") reqPath = "/index.html";
  const safePath = path.normalize(reqPath).replace(/^(\.\.[/\\])+/, "");
  const filePath = path.join(PUBLIC_DIR, safePath);

  if (!filePath.startsWith(PUBLIC_DIR)) {
    res.writeHead(403);
    res.end("Forbidden");
    return;
  }

  fs.readFile(filePath, (err, data) => {
    if (err) {
      res.writeHead(404);
      res.end("Not found");
      return;
    }
    const ext = path.extname(filePath);
    res.writeHead(200, { "Content-Type": mimeTypes[ext] || "application/octet-stream" });
    res.end(data);
  });
}

function main() {
  const server = http.createServer(serveStatic);
  const wss = new WebSocketServer({ server });

  wss.on("connection", (ws) => {
    sendJson(ws, {
      type: "event",
      name: "bridge_connected",
      ts: Date.now()
    });
  });

  const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  rl.on("line", (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    try {
      const event = JSON.parse(trimmed);
      broadcast(wss, event);
    } catch {
      // Ignore non-JSON lines from stdin.
    }
  });

  if (LOG_DIR) startLogPolling(wss, LOG_DIR);

  server.listen(PORT, () => {
    console.log(`viz bridge listening on http://localhost:${PORT}`);
    if (LOG_DIR) console.log(`polling logs from: ${LOG_DIR}`);
    console.log("stdin JSON lines are broadcast to frontend clients");
  });
}

main();
