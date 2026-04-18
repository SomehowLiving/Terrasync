# Conflict-Free Exploration with Deterministic Territory Ownership

Leaderless Rust multi-agent swarm using real FoxMQ/Vertex messaging for contention resolution and failover.

## What changed

- Removed fake local consensus module `src/vertex.rs`
- Replaced UDP peer gossip with FoxMQ MQTT topic pub/sub
- All inter-agent coordination now flows through decentralized FoxMQ topics:
  - `swarm/claims`
  - `swarm/heartbeats`
  - `swarm/ownership`
  - `swarm/events`

## Determinism model

Each agent independently computes winner from the same claim set for `(region_id, round_id)`:

1. `priority` DESC
2. `timestamp` ASC
3. `agent_id` ASC
4. `claim_id` ASC

Because all agents subscribe to the same consensus-backed FoxMQ stream and run the same ordering rule, ownership converges deterministically.

## FoxMQ/Vertex integration notes

- MQTT QoS 2 is used for coordination messages (`ExactlyOnce` in client code)
- Agent runtime has reconnect+retry logic across `--brokers` endpoints
- No central orchestrator or scheduler is introduced
- Agents remain independent processes

## Build

```bash
cargo build
```

## Run one agent

```bash
RUST_LOG=info ./target/debug/vertex-hack \
  --agent-id a1 \
  --brokers 127.0.0.1:1883 \
  --mqtt-username swarm \
  --mqtt-password swarm \
  --force-conflict-region 0,0
```

## Run demo (5 agents)

Requires a reachable FoxMQ cluster/broker endpoint:

```bash
export FOXMQ_BROKERS=127.0.0.1:1883
export FOXMQ_USERNAME=swarm
export FOXMQ_PASSWORD=swarm
./scripts/demo.sh
```

## Logs to verify

- `claims received set`
- `deterministic winner computed`
- `claim won, proceeding to explore`
- `claim lost, rerouting immediately`
- `consistent ownership digest across agents`
- `heartbeat timeout, marking failed`
- `owner failed, region reopened`

## Live Visualization (WebSocket + Canvas)

Frontend renders backend events only (no winner simulation in UI).

### Start bridge

```bash
cd viz
npm install
npm start
```

This serves UI on `http://localhost:8080` and broadcasts:

- JSON lines from `stdin` directly
- optionally parsed events from backend logs when `VIZ_LOG_DIR` is set

### Stream backend logs into UI (quick demo mode)

In a second terminal:

```bash
cd viz
VIZ_LOG_DIR=../logs/demo npm start
```

Then run your backend demo in another terminal:

```bash
./scripts/demo.sh
```

### Terminal grid (ratatui)

Same discrete grid and event overlay in the console:

```bash
cargo run --bin term-viz -- --demo
```

- **S** — toggle step mode (press **Space** / **n** for each tick)
- **+** / **-** — tick interval (slow motion, default `--tick-ms 750`)
- **K** — kill winner (during stability) or clear winner (live)
- **R** — restart scripted demo
- **Live stdin** (JSON lines, same as the web bridge): `cargo run --bin term-viz -- --live`

### Bridge event contract

- `round_update` updates food target, claim set, and winner
- `agent_position` updates cat movement targets
- `event` with `name="reroute"` flashes loser and animates away
- `event` with `name="owner_invalidated"` shows re-election pulse
