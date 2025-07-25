use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProofRecord {
    AddMember { proof: Vec<u8> },
    KeyUpdate { proof: Vec<u8> },
    RemoveMember { proof: Vec<u8> },
}
