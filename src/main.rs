mod grid;
mod types;

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use clap::Parser;
use rand::prelude::*;
use rumqttc::v5::{AsyncClient, Event, Incoming, MqttOptions};
use rumqttc::v5::mqttbytes::QoS;
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, sleep, MissedTickBehavior};
use tracing::{error, info, warn};

use crate::grid::{parse_xy, region_id};
use crate::types::{Claim, Heartbeat, Ownership, OwnershipAnnouncement, WireMessage};

const TOPIC_CLAIMS: &str = "swarm/claims";
const TOPIC_HEARTBEATS: &str = "swarm/heartbeats";
const TOPIC_OWNERSHIP: &str = "swarm/ownership";
const TOPIC_EVENTS: &str = "swarm/events";
const MIN_CLAIMS_FLOOR: usize = 3;

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long)]
    agent_id: String,
    #[arg(long, value_delimiter = ',', default_value = "127.0.0.1:1883")]
    brokers: Vec<String>,
    #[arg(long, default_value = "")]
    mqtt_username: String,
    #[arg(long, default_value = "")]
    mqtt_password: String,
    #[arg(long, default_value_t = 10)]
    grid_size: usize,
    #[arg(long, default_value_t = 500)]
    heartbeat_ms: u64,
    #[arg(long, default_value_t = 1500)]
    heartbeat_timeout_ms: u64,
    #[arg(long, default_value_t = 800)]
    claim_round_ms: u64,
    #[arg(long, default_value_t = 250)]
    tick_ms: u64,
    #[arg(long)]
    force_conflict_region: Option<String>,
    #[arg(long, default_value_t = false)]
    deterministic_priority: bool,
}

#[derive(Default)]
struct NodeState {
    claims: HashMap<String, Claim>,
    ownership: HashMap<String, Ownership>,
    last_seen: HashMap<String, u64>,
    reroute_block: HashSet<String>,
    consensus_reports: HashMap<(String, u64), HashMap<String, String>>,
    finalized_rounds: HashSet<(String, u64)>,
    decisions: HashMap<(String, u64), String>,
    re_election_active: HashSet<String>,
    round_started_logged: HashSet<u64>,
    round_closed_logged: HashSet<(String, u64)>,
}

#[derive(Clone)]
struct OutboundMessage {
    topic: &'static str,
    payload: Vec<u8>,
}

#[derive(Clone)]
struct InboundMessage {
    topic: String,
    payload: Vec<u8>,
}

#[derive(Debug, Clone)]
struct ConsensusResult {
    ownership: Ownership,
    ordered_claims: Vec<Claim>,
    digest: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .compact()
        .init();

    let args = Args::parse();
    if args.brokers.is_empty() {
        anyhow::bail!("--brokers must not be empty");
    }

    let state = Arc::new(Mutex::new(NodeState::default()));
    {
        let mut s = state.lock().await;
        s.last_seen.insert(args.agent_id.clone(), now_ms());
    }

    info!(
        agent = %args.agent_id,
        brokers = %args.brokers.join(","),
        "agent started with FoxMQ transport"
    );

    let (outbound_tx, outbound_rx) = mpsc::channel::<OutboundMessage>(4096);
    let (inbound_tx, inbound_rx) = mpsc::channel::<InboundMessage>(4096);

    let mqtt_task = tokio::spawn(mqtt_loop(args.clone(), outbound_rx, inbound_tx));
    let rx_task = tokio::spawn(rx_loop(
        inbound_rx,
        state.clone(),
        args.agent_id.clone(),
        args.claim_round_ms,
    ));
    let hb_task = tokio::spawn(heartbeat_loop(
        outbound_tx.clone(),
        state.clone(),
        args.agent_id.clone(),
        args.heartbeat_ms,
    ));
    let detect_task = tokio::spawn(detection_loop(
        outbound_tx.clone(),
        state.clone(),
        args.clone(),
    ));
    let consensus_task = tokio::spawn(consensus_loop(
        outbound_tx.clone(),
        state.clone(),
        args.clone(),
    ));
    let failover_task = tokio::spawn(failover_loop(state.clone(), args.clone()));

    let _ = tokio::join!(
        mqtt_task,
        rx_task,
        hb_task,
        detect_task,
        consensus_task,
        failover_task
    );
    Ok(())
}

async fn mqtt_loop(
    args: Args,
    mut outbound_rx: mpsc::Receiver<OutboundMessage>,
    inbound_tx: mpsc::Sender<InboundMessage>,
) {
    let mut broker_idx = 0usize;
    let mut pending: VecDeque<OutboundMessage> = VecDeque::new();

    loop {
        let broker = &args.brokers[broker_idx % args.brokers.len()];
        broker_idx = broker_idx.wrapping_add(1);
        let (host, port) = match parse_host_port(broker) {
            Some(v) => v,
            None => {
                error!(broker = %broker, "invalid broker endpoint, expected host:port");
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let client_id = format!(
            "vertex-hack-{}-{}",
            args.agent_id,
            now_ms() % 100_000
        );

        let mut options = MqttOptions::new(client_id, host, port);
        options.set_keep_alive(Duration::from_secs(5));

        if !args.mqtt_username.is_empty() {
            options.set_credentials(
                args.mqtt_username.clone(),
                args.mqtt_password.clone(),
            );
        }
        options.set_clean_start(true);

        let (client, mut eventloop) = AsyncClient::new(options, 1024);

        info!(broker = %broker, "connecting to FoxMQ broker");
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                    info!(broker = %broker, "connected to broker");
                    break;
                }
                Ok(Event::Incoming(Incoming::Disconnect(disconnect))) => {
                    warn!(
                        broker = %broker,
                        reason = ?disconnect.reason_code,
                        "broker disconnected before ConnAck"
                    );
                    sleep(Duration::from_millis(500)).await;
                    continue;
                }
                Ok(_) => {}
                Err(e) => {
                    warn!(broker = %broker, "connection failed: {e}");
                    sleep(Duration::from_millis(500)).await;
                    continue;
                }
            }
        }
        if let Err(e) = client.subscribe(TOPIC_CLAIMS, QoS::ExactlyOnce).await {
            warn!(broker = %broker, "subscribe claims failed: {e}");
            sleep(Duration::from_millis(500)).await;
            continue;
        }
        if let Err(e) = client.subscribe(TOPIC_HEARTBEATS, QoS::ExactlyOnce).await {
            warn!(broker = %broker, "subscribe heartbeats failed: {e}");
            sleep(Duration::from_millis(500)).await;
            continue;
        }
        if let Err(e) = client.subscribe(TOPIC_OWNERSHIP, QoS::ExactlyOnce).await {
            warn!(broker = %broker, "subscribe ownership failed: {e}");
            sleep(Duration::from_millis(500)).await;
            continue;
        }
        if let Err(e) = client.subscribe(TOPIC_EVENTS, QoS::ExactlyOnce).await {
            warn!(broker = %broker, "subscribe events failed: {e}");
            sleep(Duration::from_millis(500)).await;
            continue;
        }

        loop {
            while let Some(front) = pending.front().cloned() {
                match client
                    .publish(front.topic, QoS::ExactlyOnce, false, front.payload)
                    .await
                {
                    Ok(_) => {
                        pending.pop_front();
                    }
                    Err(e) => {
                        warn!(broker = %broker, "publish failed, reconnecting: {e}");
                        break;
                    }
                }
            }
            if !pending.is_empty() {
                break;
            }

            tokio::select! {
                outbound = outbound_rx.recv() => {
                    let Some(msg) = outbound else {
                        return;
                    };
                    pending.push_back(msg);
                }
                event = eventloop.poll() => {
                    match event {
                        Ok(Event::Incoming(Incoming::Publish(publish))) => {
                            if inbound_tx.send(InboundMessage {
                                topic: String::from_utf8_lossy(&publish.topic).into_owned(),
                                payload: publish.payload.to_vec(),
                            }).await.is_err() {
                                return;
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            warn!(broker = %broker, "eventloop error, reconnecting: {e}");
                            break;
                        }
                    }
                }
            }
        }

        sleep(Duration::from_millis(500)).await;
    }
}

async fn rx_loop(
    mut inbound_rx: mpsc::Receiver<InboundMessage>,
    state: Arc<Mutex<NodeState>>,
    me: String,
    claim_round_ms: u64,
) {
    while let Some(inbound) = inbound_rx.recv().await {
        let msg: WireMessage = match serde_json::from_slice(&inbound.payload) {
            Ok(v) => v,
            Err(e) => {
                warn!(topic = %inbound.topic, "bad payload: {e}");
                continue;
            }
        };
        handle_wire_message(&state, msg, &me, claim_round_ms).await;
    }
}

async fn handle_wire_message(
    state: &Arc<Mutex<NodeState>>,
    msg: WireMessage,
    me: &str,
    claim_round_ms: u64,
) {
    match msg {
        WireMessage::Heartbeat(hb) => {
            let mut s = state.lock().await;
            s.last_seen.insert(hb.agent_id, hb.last_seen);
        }
        WireMessage::Claim(claim) => {
            let mut s = state.lock().await;
            s.last_seen.insert(claim.agent_id.clone(), now_ms());
            let active_round = now_ms() / claim_round_ms;
            if s
                .decisions
                .contains_key(&(claim.region_id.clone(), claim.round_id))
            {
                warn!(
                    agent = %claim.agent_id,
                    round = claim.round_id,
                    "late_claim_ignored(agent_id, round_id)"
                );
                return;
            }
            if claim.round_id + 2 <= active_round {
                warn!(
                    agent = %claim.agent_id,
                    round = claim.round_id,
                    "late_claim_ignored(agent_id, round_id)"
                );
                return;
            }
            if !s.claims.contains_key(&claim.claim_id) {
                info!(
                    claim_id = %claim.claim_id,
                    region = %short_region(&claim.region_id),
                    round = claim.round_id,
                    from = %claim.agent_id,
                    priority = claim.priority,
                    "claim received"
                );
                s.claims.insert(claim.claim_id.clone(), claim);
            }
        }
        WireMessage::Ownership(result) => {
            let mut s = state.lock().await;
            if !result.ownership.owner_id.is_empty() {
                s.last_seen.insert(result.ownership.owner_id.clone(), now_ms());
            }

            let prev = s.ownership.get(&result.ownership.region_id).cloned();
            if prev.as_ref() != Some(&result.ownership) {
                info!(
                    region = %short_region(&result.ownership.region_id),
                    owner = %result.ownership.owner_id,
                    round = result.round_id,
                    digest = %short_region(&result.claims_digest),
                    from = %result.from,
                    "ownership update"
                );
                s.ownership
                    .insert(result.ownership.region_id.clone(), result.ownership.clone());
                if !result.ownership.owner_id.is_empty() {
                    s.re_election_active.remove(&result.ownership.region_id);
                }
            }

            s.consensus_reports
                .entry((result.ownership.region_id.clone(), result.round_id))
                .or_default()
                .insert(result.from.clone(), result.claims_digest.clone());

            if let Some(map) = s
                .consensus_reports
                .get(&(result.ownership.region_id.clone(), result.round_id))
            {
                if map.len() >= 2 {
                    if agreement(map) {
                        info!(
                            region = %short_region(&result.ownership.region_id),
                            round = result.round_id,
                            participants = map.len(),
                            digest = %short_region(&result.claims_digest),
                            "consistent ownership digest across agents"
                        );
                    } else {
                        warn!(
                            region = %short_region(&result.ownership.region_id),
                            round = result.round_id,
                            participants = map.len(),
                            "ownership digest mismatch"
                        );
                    }
                }
            }
        }
        WireMessage::ConsensusDigest {
            region_id,
            round_id,
            digest,
            from,
        } => {
            let mut s = state.lock().await;
            s.consensus_reports
                .entry((region_id.clone(), round_id))
                .or_default()
                .insert(from.clone(), digest.clone());
            if let Some(map) = s.consensus_reports.get(&(region_id.clone(), round_id)) {
                if map.len() >= 2 {
                    if agreement(map) {
                        info!(
                            region = %short_region(&region_id),
                            round = round_id,
                            participants = map.len(),
                            digest = %short_region(&digest),
                            "vertex state agreement"
                        );
                    } else {
                        warn!(
                            region = %short_region(&region_id),
                            round = round_id,
                            participants = map.len(),
                            "consensus digest mismatch"
                        );
                    }
                }
            }
            s.last_seen.insert(from, now_ms());
        }
    }

    let mut s = state.lock().await;
    s.last_seen.insert(me.to_string(), now_ms());
}

async fn heartbeat_loop(
    outbound_tx: mpsc::Sender<OutboundMessage>,
    state: Arc<Mutex<NodeState>>,
    agent_id: String,
    heartbeat_ms: u64,
) {
    let mut t = interval(Duration::from_millis(heartbeat_ms));
    t.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        t.tick().await;
        let ts = now_ms();
        {
            let mut s = state.lock().await;
            s.last_seen.insert(agent_id.clone(), ts);
        }

        publish_wire(
            &outbound_tx,
            TOPIC_HEARTBEATS,
            &WireMessage::Heartbeat(Heartbeat {
                agent_id: agent_id.clone(),
                last_seen: ts,
            }),
        )
        .await;
    }
}

async fn detection_loop(
    outbound_tx: mpsc::Sender<OutboundMessage>,
    state: Arc<Mutex<NodeState>>,
    args: Args,
) {
    let mut t = interval(Duration::from_millis(args.tick_ms));
    t.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        t.tick().await;

        let now = now_ms();
        let round = now / args.claim_round_ms;
        let target = pick_region(&state, &args).await;
        let Some((x, y, region)) = target else {
            continue;
        };

        let priority = if args.deterministic_priority {
            deterministic_priority(&args.agent_id, x, y)
        } else {
            thread_rng().gen_range(1..=1000)
        };

        let claim = Claim {
            claim_id: format!("{}:{}:{}", args.agent_id, region, round),
            agent_id: args.agent_id.clone(),
            region_id: region.clone(),
            priority,
            timestamp: now,
            round_id: round,
        };

        let should_emit = {
            let mut s = state.lock().await;
            if s.claims.contains_key(&claim.claim_id) {
                false
            } else {
                s.claims.insert(claim.claim_id.clone(), claim.clone());
                true
            }
        };

        if should_emit {
            info!(
                region = %short_region(&region),
                x,
                y,
                priority,
                round,
                "claim publish"
            );
            publish_wire(&outbound_tx, TOPIC_CLAIMS, &WireMessage::Claim(claim)).await;
        }
    }
}

async fn consensus_loop(
    outbound_tx: mpsc::Sender<OutboundMessage>,
    state: Arc<Mutex<NodeState>>,
    args: Args,
) {
    let mut t = interval(Duration::from_millis(args.tick_ms));
    t.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        t.tick().await;

        let now = now_ms();
        let active_round = now / args.claim_round_ms;
        {
            let mut s = state.lock().await;
            if s.round_started_logged.insert(active_round) {
                info!(round = active_round, "round_start");
            }
        }
        if active_round == 0 {
            continue;
        }

        let (pending, claims_snapshot, min_claims): (BTreeSet<(String, u64)>, Vec<Claim>, usize) = {
            let s = state.lock().await;
            let claims: Vec<Claim> = s.claims.values().cloned().collect();
            let mut pending = BTreeSet::new();
            for c in &claims {
                if c.round_id < active_round
                    && !s.finalized_rounds.contains(&(c.region_id.clone(), c.round_id))
                {
                    pending.insert((c.region_id.clone(), c.round_id));
                }
            }
            let alive_agents = s
                .last_seen
                .values()
                .filter(|seen| now.saturating_sub(**seen) <= args.heartbeat_timeout_ms * 2)
                .count();
            let dynamic_min = ((alive_agents * 6) + 9) / 10;
            let min_claims = MIN_CLAIMS_FLOOR.max(dynamic_min);
            (pending, claims, min_claims)
        };

        for (region, round) in pending {
            let round_close = (round + 1) * args.claim_round_ms;
            if now < round_close {
                continue;
            }

            let scoped_count = claims_snapshot
                .iter()
                .filter(|c| c.region_id == region && c.round_id == round)
                .count();
            {
                let mut s = state.lock().await;
                if s.round_closed_logged.insert((region.clone(), round)) {
                    info!(
                        region = %short_region(&region),
                        round,
                        claims = scoped_count,
                        "round_close"
                    );
                }
            }

            if scoped_count < min_claims {
                continue;
            }

            {
                let s = state.lock().await;
                if s.finalized_rounds.contains(&(region.clone(), round)) {
                    continue;
                }
            }

            let Some(result) = resolve_region(&region, round, &claims_snapshot, now) else {
                continue;
            };

            info!(
                region = %short_region(&region),
                round,
                claims = %format_claim_ids(&result.ordered_claims),
                "final_claim_set"
            );

            info!(
                region = %short_region(&region),
                round,
                order = %format_order(&result.ordered_claims),
                winner = %result.ownership.owner_id,
                "winner"
            );

            let should_execute = {
                let mut s = state.lock().await;
                if s.finalized_rounds.contains(&(region.clone(), round)) {
                    false
                } else {
                    s.decisions
                        .entry((region.clone(), round))
                        .or_insert_with(|| result.ownership.owner_id.clone());
                    let decided_winner = s
                        .decisions
                        .get(&(region.clone(), round))
                        .cloned()
                        .unwrap_or_default();
                    let prev_owner = s
                        .ownership
                        .get(&region)
                        .map(|o| o.owner_id.clone())
                        .unwrap_or_default();
                s.ownership.insert(region.clone(), result.ownership.clone());
                    if decided_winner != args.agent_id {
                        s.reroute_block.insert(region.clone());
                    } else {
                        s.reroute_block.remove(&region);
                        s.re_election_active.remove(&region);
                    }
                    s.finalized_rounds.insert((region.clone(), round));
                    prev_owner != decided_winner && decided_winner == args.agent_id
                }
            };

            if should_execute {
                info!(
                    region = %short_region(&region),
                    "claim won, proceeding to explore"
                );
            } else if result.ownership.owner_id != args.agent_id {
                info!(
                    region = %short_region(&region),
                    winner = %result.ownership.owner_id,
                    "claim lost, rerouting immediately"
                );
            }

            publish_wire(
                &outbound_tx,
                TOPIC_OWNERSHIP,
                &WireMessage::Ownership(OwnershipAnnouncement {
                    ownership: result.ownership.clone(),
                    round_id: round,
                    claims_digest: result.digest.clone(),
                    from: args.agent_id.clone(),
                }),
            )
            .await;

            publish_wire(
                &outbound_tx,
                TOPIC_EVENTS,
                &WireMessage::ConsensusDigest {
                    region_id: region,
                    round_id: round,
                    digest: result.digest,
                    from: args.agent_id.clone(),
                },
            )
            .await;
        }
    }
}

async fn failover_loop(state: Arc<Mutex<NodeState>>, args: Args) {
    let mut t = interval(Duration::from_millis(args.tick_ms));
    t.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        t.tick().await;
        let now = now_ms();

        let mut s = state.lock().await;
        let watched_owners: HashSet<String> = s
            .ownership
            .values()
            .filter(|o| !o.owner_id.is_empty())
            .map(|o| o.owner_id.clone())
            .collect();

        let mut stale = HashSet::new();
        for owner in watched_owners {
            if owner == args.agent_id {
                continue;
            }
            let Some(last) = s.last_seen.get(&owner) else {
                continue;
            };
            if now.saturating_sub(*last) > args.heartbeat_timeout_ms {
                stale.insert(owner.clone());
            }
        }

        if stale.is_empty() {
            continue;
        }

        for dead in &stale {
            warn!(agent = %dead, "heartbeat_missed(agent_id)");
        }

        let mut reopened = Vec::new();
        let regions_to_reopen: Vec<String> = s
            .ownership
            .values()
            .filter(|o| stale.contains(&o.owner_id))
            .map(|o| o.region_id.clone())
            .filter(|r| !s.re_election_active.contains(r))
            .collect();
        for region in &regions_to_reopen {
            s.re_election_active.insert(region.clone());
        }
        for ownership in s.ownership.values_mut() {
            if stale.contains(&ownership.owner_id) && regions_to_reopen.contains(&ownership.region_id)
            {
                warn!(
                    region = %short_region(&ownership.region_id),
                    dead_owner = %ownership.owner_id,
                    "owner_invalidated(region_id)"
                );
                info!(
                    region = %short_region(&ownership.region_id),
                    round_id = now / args.claim_round_ms,
                    "re_election_started(region_id, round_id)"
                );
                ownership.owner_id.clear();
                ownership.last_updated = now;
                reopened.push(ownership.region_id.clone());
            }
        }

        for region in reopened {
            s.reroute_block.remove(&region);
        }

        for dead in stale {
            s.last_seen.remove(&dead);
        }
    }
}

async fn pick_region(state: &Arc<Mutex<NodeState>>, args: &Args) -> Option<(usize, usize, String)> {
    let force = args
        .force_conflict_region
        .as_ref()
        .and_then(|s| parse_xy(s))
        .unwrap_or((0, 0));

    let (preferred_x, preferred_y) = force;
    let preferred = region_id(preferred_x, preferred_y);

    {
        let s = state.lock().await;
        let owner = s
            .ownership
            .get(&preferred)
            .map(|o| o.owner_id.clone())
            .unwrap_or_default();
        if (owner.is_empty() || owner == args.agent_id) && !s.reroute_block.contains(&preferred) {
            return Some((preferred_x, preferred_y, preferred));
        }
    }

    let s = state.lock().await;
    for x in 0..args.grid_size {
        for y in 0..args.grid_size {
            let r = region_id(x, y);
            let owner = s
                .ownership
                .get(&r)
                .map(|o| o.owner_id.clone())
                .unwrap_or_default();
            if owner.is_empty() && !s.reroute_block.contains(&r) {
                return Some((x, y, r));
            }
        }
    }

    None
}

async fn publish_wire(outbound_tx: &mpsc::Sender<OutboundMessage>, topic: &'static str, msg: &WireMessage) {
    let payload = match serde_json::to_vec(msg) {
        Ok(v) => v,
        Err(e) => {
            error!(topic = %topic, "serialize error: {e}");
            return;
        }
    };

    if outbound_tx
        .send(OutboundMessage { topic, payload })
        .await
        .is_err()
    {
        error!(topic = %topic, "failed to enqueue outbound message");
    }
}

fn parse_host_port(s: &str) -> Option<(String, u16)> {
    let (host, port_str) = s.rsplit_once(':')?;
    let port = port_str.parse::<u16>().ok()?;
    Some((host.to_string(), port))
}

fn resolve_region(region_id: &str, round_id: u64, claims: &[Claim], now: u64) -> Option<ConsensusResult> {
    let mut scoped: Vec<Claim> = claims
        .iter()
        .filter(|c| c.region_id == region_id && c.round_id == round_id)
        .cloned()
        .collect();

    if scoped.is_empty() {
        return None;
    }

    scoped.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(a.agent_id.cmp(&b.agent_id))
            .then(a.claim_id.cmp(&b.claim_id))
    });

    let owner = scoped[0].agent_id.clone();

    let mut hasher = Sha256::new();
    hasher.update(region_id.as_bytes());
    hasher.update(round_id.to_le_bytes());
    for claim in &scoped {
        hasher.update(claim.claim_id.as_bytes());
    }
    let digest = hex::encode(hasher.finalize());

    Some(ConsensusResult {
        ownership: Ownership {
            region_id: region_id.to_string(),
            owner_id: owner,
            last_updated: now,
        },
        ordered_claims: scoped,
        digest,
    })
}

fn agreement(reports: &HashMap<String, String>) -> bool {
    if reports.is_empty() {
        return true;
    }
    let mut it = reports.values();
    let Some(first) = it.next() else {
        return true;
    };
    it.all(|v| v == first)
}

fn deterministic_priority(agent_id: &str, x: usize, y: usize) -> u64 {
    let mut h = Sha256::new();
    h.update(agent_id.as_bytes());
    h.update(format!("{x}:{y}").as_bytes());
    let bytes = h.finalize();
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(arr)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_millis(0))
        .as_millis() as u64
}

fn short_region(id: &str) -> String {
    id.chars().take(8).collect::<String>()
}

fn format_order(claims: &[Claim]) -> String {
    claims
        .iter()
        .map(|c| format!("{}:{}", c.agent_id, c.priority))
        .collect::<Vec<_>>()
        .join(" > ")
}

fn format_claim_ids(claims: &[Claim]) -> String {
    claims
        .iter()
        .map(|c| c.claim_id.clone())
        .collect::<Vec<_>>()
        .join(",")
}
