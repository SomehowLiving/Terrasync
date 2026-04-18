use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Claim {
    pub claim_id: String,
    pub agent_id: String,
    pub region_id: String,
    pub priority: u64,
    pub timestamp: u64,
    pub round_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Ownership {
    pub region_id: String,
    pub owner_id: String,
    pub last_updated: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Heartbeat {
    pub agent_id: String,
    pub last_seen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct OwnershipAnnouncement {
    pub ownership: Ownership,
    pub round_id: u64,
    pub claims_digest: String,
    pub from: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireMessage {
    Heartbeat(Heartbeat),
    Claim(Claim),
    Ownership(OwnershipAnnouncement),
    ConsensusDigest {
        region_id: String,
        round_id: u64,
        digest: String,
        from: String,
    },
}
