# 🚀 Terrasync

## Conflict-Free Exploration with Deterministic Territory Ownership

A leaderless multi-agent system where agents compete for shared resources and resolve conflicts **deterministically** — without coordination servers, locks, or duplicate work.

---

# 🧠 Problem

In distributed multi-agent systems (robots, AI agents, services), multiple agents often detect the same task or region at the same time.

This leads to:

* duplicate work
* resource contention
* inefficient coordination
* reliance on central schedulers

Most systems solve this with:

* locks
* leaders
* or probabilistic backoff

👉 None guarantee:

> **exactly one owner per task — deterministically — without central control**

---

# 💡 Solution

Terrasync turns **contention into a consensus problem**.

Instead of avoiding conflicts, agents:

1. **compete for regions**
2. **broadcast claims**
3. **independently compute the same winner**

Result:

* exactly one owner per region
* no duplicate execution
* immediate rerouting for losers

---

# ⚙️ How It Works

## 1. Region Abstraction

Environment is divided into regions:

```text
(x, y) → region_id = hash(x, y)
```

---

## 2. Claim Broadcast

Agents publish claims via FoxMQ:

* agent_id
* region_id
* priority
* timestamp

Topics used:

* `swarm/claims`
* `swarm/heartbeats`
* `swarm/ownership`
* `swarm/events`

---

## 3. Deterministic Consensus

Each agent computes the same winner using:

1. `priority` DESC
2. `timestamp` ASC
3. `agent_id` ASC
4. `claim_id` ASC

Because:

* all agents receive the same message stream (FoxMQ / Vertex)
* all run identical logic

👉 **ownership converges without coordination**

---

## 4. Conflict-Free Execution

* Winner → proceeds
* Others → instantly reroute

No collisions. No duplication.

---

## 5. Failure Recovery

If the owner dies:

* heartbeat timeout triggers
* region becomes unclaimed
* new claim round begins

---

# 🧩 Key Properties

* **Leaderless** — no central orchestrator
* **Deterministic** — same inputs → same outcome
* **Conflict-free** — duplicate work eliminated
* **Fault-tolerant** — automatic recovery
* **Scalable** — multi-region, multi-agent

---

# 🏗️ Architecture

### Agent Node (Rust)

* claim generation
* consensus computation
* rerouting logic
* heartbeat monitoring

### Messaging Layer

* FoxMQ (MQTT pub/sub)
* Vertex-backed ordering (deterministic message stream)

### Topics

* `swarm/claims`
* `swarm/heartbeats`
* `swarm/ownership`
* `swarm/events`

### Observability

* terminal dashboard (real-time)
* optional web visualization

---

# 🔬 Determinism Model

Each agent independently computes ownership for `(region_id, round_id)`.

No coordination step exists.

```text
same claim set → same ordering → same winner
```

This guarantees convergence even under partial visibility.

---

# 🧪 Demo Flow

1. Start 5 agents
2. All detect the same region
3. All submit claims simultaneously
4. System resolves **one winner deterministically**
5. Others reroute instantly

Then:

6. Kill the winner
7. Heartbeat timeout triggers
8. Region reopens
9. New winner is selected automatically

---

# 🛠️ Build

```bash
cargo build
```

---

# ▶️ Run Single Agent

```bash
RUST_LOG=info ./target/debug/vertex-hack \
  --agent-id a1 \
  --brokers 127.0.0.1:1883 \
  --mqtt-username swarm \
  --mqtt-password swarm \
  --force-conflict-region 0,0
```

---

# ▶️ Run Full Demo (5 Agents)

```bash
export FOXMQ_BROKERS=127.0.0.1:1883
export FOXMQ_USERNAME=swarm
export FOXMQ_PASSWORD=swarm

./scripts/demo.sh
```

---

# 📊 Logs to Verify

Look for:

* `claims received set`
* `deterministic winner computed`
* `claim won, proceeding to explore`
* `claim lost, rerouting immediately`
* `heartbeat timeout`
* `owner failed, region reopened`

---

# 🖥️ Visualization

## Terminal Dashboard

```bash
cargo run --bin term-viz -- --live
```

* shows agents, claims, winner, events
* supports commands:

  * `kill a1`
  * `kill winner`

---

## Web Visualization (Optional)

```bash
cd viz
npm install
npm start
```

Open:

```
http://localhost:8080
```

Stream logs:

```bash
VIZ_LOG_DIR=../logs/demo npm start
```

---

# 🎯 What This Demonstrates

* real-time multi-agent contention
* deterministic consensus without leaders
* zero-duplication task allocation
* automatic recovery under failure

---

# 🏁 Summary

> Terrasync eliminates coordination complexity by making ownership deterministic — turning distributed contention into a predictable, convergent system.
