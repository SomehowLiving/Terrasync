## Product Requirements Document (PRD)

### Project: Conflict-Free Exploration with Deterministic Territory Ownership

---

## 1. Product Summary

A decentralized swarm coordination system where multiple agents (robots / drones / AI agents) **compete for territory**, resolve conflicts using **Vertex consensus**, and **guarantee single ownership per region** without any central controller.

Core value:

> Deterministic, leaderless task allocation under simultaneous contention.

---

## 2. Goals

### Primary Goal

Demonstrate **conflict-free, decentralized coordination** using Vertex:

* No duplicate work
* No central orchestrator
* Deterministic ownership

### Secondary Goals

* Show **fault tolerance (node failure → recovery)**
* Show **low-latency coordination**
* Provide **clear, visual demo of swarm behavior**

---

## 3. Non-Goals

* High-fidelity robotics / SLAM
* Real-world hardware integration
* Complex UI / frontend polish
* AI-heavy decision making

Focus is **coordination logic**, not perception.

---

## 4. Target Track Fit

* **Primary:** Track 1 (Ghost in the Machine)
* **Secondary (optional upgrade):** Track 3

---

## 5. Core Features

### 5.1 Peer Discovery & Handshake

* Agents auto-discover via Vertex
* Establish session
* Exchange identity + metadata

---

### 5.2 Heartbeat System

* Periodic liveness signals
* Detect stale / dead nodes
* Trigger reallocation

---

### 5.3 Region Abstraction

* Environment divided into grid
* Each cell = `region_id = hash(x, y)`
* State:

  * unexplored
  * claimed
  * completed

---

### 5.4 Claim Broadcasting

Agents publish:

* agent_id
* region_id
* priority_score
* timestamp

---

### 5.5 Deterministic Ownership (CORE)

* Multiple claims → Vertex consensus
* Single owner selected deterministically
* All agents agree on same result

---

### 5.6 Rerouting Logic

* Losing agents:

  * abandon region
  * select next best region
  * re-enter claim cycle

---

### 5.7 Failure Recovery

* Owner node dies
* Heartbeat timeout triggers:

  * region becomes unclaimed
  * new claim round

---

### 5.8 Shared State (Swarm Memory)

* Region ownership map
* Agent statuses
* Last seen timestamps

---

### 5.9 Observability (IMPORTANT)

* Live view of:

  * agents
  * claims
  * winners
  * failures
  * reroutes

---

## 6. System Architecture

### Components

#### Agent Node

* discovery module
* claim generator
* consensus listener
* execution engine
* heartbeat sender

---

#### Vertex Layer

* peer discovery
* consensus (DAG + ordering)
* state agreement

---

#### FoxMQ (optional)

* pub/sub layer for claims + updates

---

#### Simulation Layer

* grid world
* agent movement
* visualization

---

## 7. User Flow (Demo Flow)

1. Start 5–10 agents
2. Agents discover each other
3. Shared map initialized
4. Same region detected by multiple agents
5. All send claims
6. Vertex resolves winner
7. Winner proceeds
8. Others reroute
9. Kill winner
10. Region reassigned

---

## 8. Phase Plan

---

## Phase 1 — **Core Coordination (MVP)**

### Objective

Prove:

* P2P communication
* deterministic ownership
* basic rerouting

### Features

* 2–3 agents
* peer discovery
* heartbeat
* simple grid (5x5)
* claim → consensus → winner
* reroute logic

### Deliverable

* terminal logs OR simple visualization
* proof of:

  * discovery
  * claim conflict
  * single winner

### Success Criteria

* no duplicate ownership
* consistent winner across all agents
* <1s resolution

---

## Phase 2 — **Swarm Behavior + Scaling**

### Objective

Show realistic swarm coordination

### Features

* 5–10 agents
* multiple regions
* continuous exploration loop
* dynamic rerouting
* shared state replication

### Enhancements

* priority scoring:

  * distance
  * random tie-break
* multiple simultaneous conflicts

### Deliverable

* simulation with visible movement
* multiple claim cycles

### Success Criteria

* zero collisions
* efficient coverage
* consistent state across agents

---

## Phase 3 — **Resilience & Fault Tolerance (CRITICAL)**

### Objective

Win on robustness (major judging factor)

### Features

* heartbeat timeout detection
* node failure simulation
* automatic reallocation
* stale ownership cleanup

### Chaos Testing

* kill agent mid-task
* introduce latency
* simulate message delay

### Deliverable

* demo showing:

  * failure
  * recovery
  * re-election

### Success Criteria

* no stuck regions
* system continues operating
* ownership always valid

---

## Phase 4 — **Observability & Demo Polish**

### Objective

Make behavior obvious to judges

### Features

* dashboard / visualization:

  * grid map
  * agent positions
  * ownership colors
* event logs:

  * claims
  * winners
  * failures

### Deliverable

* clean demo UI OR CLI visualization

### Success Criteria

* judges understand system in <30 seconds

---

## Phase 5 — **Advanced Differentiation (Optional but Powerful)**

### Objective

Push into top-tier submissions

### Features

#### 1. Ownership Leasing

* region ownership expires after time
* enables dynamic rebalancing

#### 2. Priority Dynamics

* energy-aware selection
* distance-based optimization

#### 3. Multi-Region Coordination

* agents batch claims
* optimize pathing

#### 4. Hybrid Agents

* mix:

  * explorers
  * relays
  * coordinators

---

## 9. Technical Stack (Suggested)

* **Language:** Python or Node.js
* **Vertex:** core consensus layer
* **FoxMQ:** messaging layer
* **Simulation:** simple grid (custom or Webots)
* **Visualization:** lightweight (Canvas / CLI / Web UI)

---

## 10. Key Metrics

* Conflict resolution latency (<100–500ms target)
* % duplicate work (target: 0%)
* recovery time after failure
* consistency (all nodes agree)

---

## 11. Risks

| Risk                          | Mitigation                 |
| ----------------------------- | -------------------------- |
| Vertex integration complexity | start with warmup first    |
| inconsistent state            | strict deterministic rules |
| unclear demo                  | invest in visualization    |
| over-engineering              | stick to phases            |

---

## 12. What NOT to Do

* Don’t build a UI-heavy app
* Don’t overcomplicate AI logic
* Don’t rely on a central server
* Don’t skip failure scenarios

---

## 13. Final Positioning

Pitch it as:

> “A leaderless, deterministic coordination protocol for conflict-free task allocation in autonomous swarms.”

Not:

* “robot simulation”
* “multi-agent demo”

---

## Final Execution Advice

Focus order:

1. **Consensus + conflict resolution**
2. **Failure recovery**
3. **Clear demo**
4. **Polish**

If you nail those three:
**this becomes a serious winning contender in Track 1.**
