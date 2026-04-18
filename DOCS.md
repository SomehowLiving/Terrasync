## Architecture (Refactored)

### Runtime loops per agent

Each agent process runs:

- FoxMQ transport loop: MQTT subscribe/publish with reconnect retry
- RX loop: deserializes topic payloads into swarm events
- Heartbeat loop: publishes liveness to `swarm/heartbeats`
- Detection loop: emits region claims to `swarm/claims`
- Consensus loop: finalizes deterministic owner and publishes to `swarm/ownership`
- Failover loop: reopens regions when owner heartbeat times out

### Topic mapping

- `swarm/claims`: `WireMessage::Claim`
- `swarm/heartbeats`: `WireMessage::Heartbeat`
- `swarm/ownership`: `WireMessage::Ownership`
- `swarm/events`: digest/events (`WireMessage::ConsensusDigest`)

### Deterministic ownership

For each finalized round:

- collect all claims for `(region_id, round_id)`
- sort by `priority DESC`, `timestamp ASC`, `agent_id ASC`, `claim_id ASC`
- first element becomes owner
- compute digest from ordered claim IDs
- broadcast ownership + digest

### Consistency evidence

Agents log:

- claim set snapshot per region/round (`claims received set`)
- independently computed winner (`deterministic winner computed`)
- digest agreement from peer ownership broadcasts (`consistent ownership digest across agents`)

### Failure handling

- owner heartbeat missing beyond timeout -> mark failed
- region ownership cleared (`owner failed, region reopened`)
- new claim round reallocates deterministically
