/**
 * Discrete 10×10 grid visualization — logical positions are integers only.
 * Smooth animation interpolates between cell centers (Manhattan paths).
 */

const GRID_W = 10;
const GRID_H = 10;

const COLORS = [
  "#3b82f6", "#22c55e", "#eab308", "#a855f7", "#ec4899",
  "#06b6d4", "#f97316", "#84cc16", "#6366f1", "#14b8a6"
];

const canvas = document.getElementById("canvas");
const ctx = canvas.getContext("2d");
const statusEl = document.getElementById("status");
const legendEl = document.getElementById("legend");

let pad = 24;
let cellSize = 40;

/** @type {{ x: number, y: number } | null} */
let targetCell = null;
/** @type {string | null} */
let winnerId = null;
/** @type {Map<string, { id: string, color: string, cellX: number, cellY: number, anim: { from: {x:number,y:number}, to: {x:number,y:number}, t: number } | null, path: {x:number,y:number}[], loserFlashUntil: number }>} */
const agents = new Map();

let lastTs = performance.now();
const MOVE_SPEED = 4.5;

function clampCell(x, y) {
  return {
    x: Math.max(0, Math.min(GRID_W - 1, Math.floor(x))),
    y: Math.max(0, Math.min(GRID_H - 1, Math.floor(y)))
  };
}

function cellKey(x, y) {
  return `${x},${y}`;
}

/** Manhattan path: horizontal first, then vertical (no diagonals). */
function manhattanPath(x0, y0, x1, y1) {
  const path = [{ x: x0, y: y0 }];
  let x = x0;
  let y = y0;
  while (x !== x1) {
    x += x < x1 ? 1 : -1;
    path.push({ x, y });
  }
  while (y !== y1) {
    y += y < y1 ? 1 : -1;
    path.push({ x, y });
  }
  return path;
}

/** Center of cell (gx, gy) in canvas pixels. */
function cellCenterPx(gx, gy) {
  return {
    x: pad + gx * cellSize + cellSize / 2,
    y: pad + gy * cellSize + cellSize / 2
  };
}

function resizeCanvas() {
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  canvas.width = Math.floor(w * dpr);
  canvas.height = Math.floor(h * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const availW = w - pad * 2;
  const availH = h - pad * 2;
  cellSize = Math.min(availW / GRID_W, availH / GRID_H);
  pad = Math.max(16, (w - cellSize * GRID_W) / 2);
}

function getOrCreateAgent(id) {
  let a = agents.get(id);
  if (!a) {
    const c = COLORS[agents.size % COLORS.length];
    const start = clampCell(Math.random() * GRID_W, Math.random() * GRID_H);
    a = {
      id,
      color: c,
      cellX: start.x,
      cellY: start.y,
      anim: null,
      path: [],
      loserFlashUntil: 0
    };
    agents.set(id, a);
    rebuildLegend();
  }
  return a;
}

function rebuildLegend() {
  legendEl.innerHTML = "";
  for (const a of agents.values()) {
    const row = document.createElement("div");
    row.className = "legend-row";
    const sw = document.createElement("span");
    sw.className = "swatch";
    sw.style.background = a.color;
    const lab = document.createElement("span");
    lab.textContent = a.id + (winnerId === a.id ? " (winner)" : "");
    row.appendChild(sw);
    row.appendChild(lab);
    legendEl.appendChild(row);
  }
}

/** Start animating along path: path[0] should equal current cell; pop first or build from current. */
function setAgentPath(agent, pathCells) {
  if (!pathCells.length) return;
  const cur = { x: agent.cellX, y: agent.cellY };
  let i = 0;
  if (pathCells[0].x === cur.x && pathCells[0].y === cur.y) i = 1;
  agent.path = pathCells.slice(i);
  if (agent.path.length === 0) {
    agent.anim = null;
    return;
  }
  const next = agent.path[0];
  agent.anim = {
    from: { x: cur.x, y: cur.y },
    to: { x: next.x, y: next.y },
    t: 0
  };
}

function advanceAnim(agent, dt) {
  if (!agent.anim) return;
  agent.anim.t += dt * MOVE_SPEED;
  if (agent.anim.t < 1) return;
  agent.anim.t = 1;
  agent.cellX = agent.anim.to.x;
  agent.cellY = agent.anim.to.y;
  agent.path.shift();
  if (agent.path.length === 0) {
    agent.anim = null;
    return;
  }
  const next = agent.path[0];
  agent.anim = {
    from: { x: agent.cellX, y: agent.cellY },
    to: { x: next.x, y: next.y },
    t: 0
  };
}

/** Pixel position for drawing (interpolated between cell centers). */
function agentDrawPos(agent) {
  const from = cellCenterPx(agent.anim ? agent.anim.from.x : agent.cellX, agent.anim ? agent.anim.from.y : agent.cellY);
  const to = cellCenterPx(agent.anim ? agent.anim.to.x : agent.cellX, agent.anim ? agent.anim.to.y : agent.cellY);
  const t = agent.anim ? agent.anim.t : 1;
  return {
    x: from.x + (to.x - from.x) * t,
    y: from.y + (to.y - from.y) * t
  };
}

/** Occupied cells by other agents' logical positions (for reroute). */
function occupiedByOthers(excludeId) {
  const set = new Set();
  for (const a of agents.values()) {
    if (a.id === excludeId) continue;
    set.add(cellKey(a.cellX, a.cellY));
    if (a.anim) {
      set.add(cellKey(a.anim.to.x, a.anim.to.y));
    }
  }
  return set;
}

/** Find distinct cells for losers: spiral Manhattan distance from target. */
function allocateLoserCells(tx, ty, loserIds) {
  const occupied = new Set();
  occupied.add(cellKey(tx, ty));
  for (const id of agents.keys()) {
    const a = agents.get(id);
    occupied.add(cellKey(a.cellX, a.cellY));
  }
  const result = new Map();
  const candidates = [];
  for (let d = 1; d < Math.max(GRID_W, GRID_H) * 2; d++) {
    for (let dx = -d; dx <= d; dx++) {
      for (let dy = -d; dy <= d; dy++) {
        if (Math.abs(dx) + Math.abs(dy) !== d) continue;
        const nx = tx + dx;
        const ny = ty + dy;
        if (nx < 0 || nx >= GRID_W || ny < 0 || ny >= GRID_H) continue;
        if (nx === tx && ny === ty) continue;
        candidates.push({ x: nx, y: ny });
      }
    }
  }
  let ci = 0;
  for (const lid of loserIds) {
    while (ci < candidates.length) {
      const { x, y } = candidates[ci++];
      const k = cellKey(x, y);
      if (!occupied.has(k)) {
        occupied.add(k);
        result.set(lid, { x, y });
        break;
      }
    }
  }
  return result;
}

function applyRoundUpdate(ev) {
  const t = ev.target || ev.region;
  if (t && typeof t.x === "number" && typeof t.y === "number") {
    targetCell = clampCell(t.x, t.y);
  }
  const claims = Array.isArray(ev.claims) ? ev.claims : [];
  const roundWinner = ev.winner || null;

  for (const id of claims) {
    getOrCreateAgent(id);
  }

  winnerId = roundWinner;

  if (!targetCell) return;

  const tx = targetCell.x;
  const ty = targetCell.y;

  if (roundWinner && claims.includes(roundWinner)) {
    const losers = claims.filter((id) => id !== roundWinner);
    const loserDest = allocateLoserCells(tx, ty, losers);

    const w = getOrCreateAgent(roundWinner);
    const wpath = manhattanPath(w.cellX, w.cellY, tx, ty);
    setAgentPath(w, wpath);

    for (const lid of losers) {
      const a = getOrCreateAgent(lid);
      const dest = loserDest.get(lid);
      if (!dest) continue;
      const p = manhattanPath(a.cellX, a.cellY, dest.x, dest.y);
      setAgentPath(a, p);
      a.loserFlashUntil = performance.now() + 400;
    }

    for (const id of claims) {
      if (id === roundWinner || losers.includes(id)) continue;
      const a = getOrCreateAgent(id);
      const p = manhattanPath(a.cellX, a.cellY, tx, ty);
      setAgentPath(a, p);
    }
  } else {
    for (const id of claims) {
      const a = getOrCreateAgent(id);
      const p = manhattanPath(a.cellX, a.cellY, tx, ty);
      setAgentPath(a, p);
    }
  }

  rebuildLegend();
}

function applyPosition(ev) {
  const id = ev.agent_id;
  if (!id) return;
  const a = getOrCreateAgent(id);
  const c = clampCell(ev.x, ev.y);
  a.path = [];
  a.anim = null;
  a.cellX = c.x;
  a.cellY = c.y;
}

function applyEvent(ev) {
  if (ev.name === "reroute" && ev.agent_id) {
    const a = getOrCreateAgent(ev.agent_id);
    if (!targetCell) return;
    const tx = targetCell.x;
    const ty = targetCell.y;
    const occ = occupiedByOthers(a.id);
    let dest = null;
    for (let d = 1; d < 12; d++) {
      for (let dx = -d; dx <= d; dx++) {
        for (let dy = -d; dy <= d; dy++) {
          if (Math.abs(dx) + Math.abs(dy) !== d) continue;
          const nx = tx + dx;
          const ny = ty + dy;
          if (nx < 0 || nx >= GRID_W || ny < 0 || ny >= GRID_H) continue;
          if (nx === tx && ny === ty) continue;
          const k = cellKey(nx, ny);
          if (!occ.has(k)) {
            dest = { x: nx, y: ny };
            break;
          }
        }
        if (dest) break;
      }
      if (dest) break;
    }
    if (!dest) dest = clampCell(a.cellX === tx ? a.cellX + 1 : tx, a.cellY);
    const p = manhattanPath(a.cellX, a.cellY, dest.x, dest.y);
    setAgentPath(a, p);
  }
  if (ev.name === "owner_invalidated") {
    winnerId = null;
    if (Array.isArray(ev.region) && ev.region.length >= 2) {
      targetCell = clampCell(ev.region[0], ev.region[1]);
    }
  }
}

function handleMessage(data) {
  if (data.type === "round_update") {
    applyRoundUpdate(data);
    statusEl.textContent = `Round ${data.round ?? "?"} · target (${targetCell?.x ?? "?"},${targetCell?.y ?? "?"}) · winner ${data.winner ?? "—"}`;
    return;
  }
  if (data.type === "agent_position") {
    applyPosition(data);
    return;
  }
  if (data.type === "event") {
    applyEvent(data);
    return;
  }
}

function connect() {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  const ws = new WebSocket(`${proto}//${location.host}`);
  ws.onopen = () => {
    statusEl.textContent = "Connected — waiting for events…";
  };
  ws.onmessage = (e) => {
    try {
      handleMessage(JSON.parse(e.data));
    } catch (_) {}
  };
  ws.onclose = () => {
    statusEl.textContent = "Disconnected — retrying…";
    setTimeout(connect, 2000);
  };
}

function drawGrid() {
  ctx.fillStyle = "#0f172a";
  ctx.fillRect(0, 0, canvas.clientWidth, canvas.clientHeight);

  const gw = cellSize * GRID_W;
  const gh = cellSize * GRID_H;
  const ox = pad;
  const oy = pad;

  ctx.strokeStyle = "rgba(148,163,184,0.35)";
  ctx.lineWidth = 1;
  for (let i = 0; i <= GRID_W; i++) {
    const x = ox + i * cellSize;
    ctx.beginPath();
    ctx.moveTo(x, oy);
    ctx.lineTo(x, oy + gh);
    ctx.stroke();
  }
  for (let j = 0; j <= GRID_H; j++) {
    const y = oy + j * cellSize;
    ctx.beginPath();
    ctx.moveTo(ox, y);
    ctx.lineTo(ox + gw, y);
    ctx.stroke();
  }

  if (targetCell) {
    const fx = ox + targetCell.x * cellSize;
    const fy = oy + targetCell.y * cellSize;
    ctx.fillStyle = "rgba(34,197,94,0.22)";
    ctx.fillRect(fx, fy, cellSize, cellSize);
    ctx.strokeStyle = "rgba(34,197,94,0.7)";
    ctx.lineWidth = 2;
    ctx.strokeRect(fx + 1, fy + 1, cellSize - 2, cellSize - 2);
    ctx.fillStyle = "#86efac";
    ctx.font = `${Math.max(10, cellSize * 0.28)}px system-ui`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText("food", fx + cellSize / 2, fy + cellSize / 2);
  }
}

function drawAgents() {
  const radius = Math.max(6, cellSize * 0.22);
  for (const a of agents.values()) {
    const pos = agentDrawPos(a);
    ctx.beginPath();
    ctx.arc(pos.x, pos.y, radius, 0, Math.PI * 2);
    ctx.fillStyle = a.color;
    ctx.fill();
    ctx.strokeStyle = winnerId === a.id ? "#fbbf24" : "rgba(15,23,42,0.85)";
    ctx.lineWidth = winnerId === a.id ? 3 : 1.5;
    ctx.stroke();
    ctx.fillStyle = "#0f172a";
    ctx.font = `${Math.max(9, cellSize * 0.2)}px system-ui`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(a.id, pos.x, pos.y);
    if (performance.now() < a.loserFlashUntil) {
      ctx.strokeStyle = "rgba(248,113,113,0.9)";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(pos.x, pos.y, radius + 4, 0, Math.PI * 2);
      ctx.stroke();
    }
  }
}

function frame(ts) {
  const dt = Math.min(0.05, (ts - lastTs) / 1000);
  lastTs = ts;
  for (const a of agents.values()) {
    advanceAnim(a, dt);
  }

  drawGrid();
  drawAgents();
  requestAnimationFrame(frame);
}

window.addEventListener("resize", () => {
  resizeCanvas();
});

resizeCanvas();
connect();
requestAnimationFrame(frame);
