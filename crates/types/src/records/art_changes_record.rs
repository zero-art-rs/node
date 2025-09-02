use ark_ec::AffineRepr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::types::BranchChanges;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use uuid::Uuid;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound = "")]
pub struct ARTChangesRecord<G>
where
    G: AffineRepr + CanonicalSerialize + CanonicalDeserialize,
{
    /// ART changes
    pub changes: BranchChanges<G>, //BranchChanges<G>,
    pub created_at: DateTime<Utc>,
    pub sequence_number: i64,
    pub chat_id: Uuid,
    #[serde_as(as = "Option<Base64>")]
    pub metadata: Option<Vec<u8>>,
    #[serde_as(as = "Option<Base64>")]
    pub payload: Option<Vec<u8>>,
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ARTChangesOutboxRecord {
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
    pub created_at: DateTime<Utc>,
    pub sequence_number: i64,
    pub chat_id: Uuid,
    #[serde_as(as = "Option<Base64>")]
    pub metadata: Option<Vec<u8>>,
    #[serde_as(as = "Option<Base64>")]
    pub payload: Option<Vec<u8>>,
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
}

impl<G> ARTChangesRecord<G>
where
    G: AffineRepr + CanonicalSerialize + CanonicalDeserialize,
{
    pub fn new(
        data: BranchChanges<G>,
        sequence_number: i64,
        chat_id: Uuid,
        metadata: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        proof: Vec<u8>,
    ) -> Self {
        Self {
            changes: data,
            created_at: Utc::now(),
            sequence_number,
            chat_id,
            metadata,
            payload,
            proof,
        }
    }
}

impl ARTChangesOutboxRecord {
    pub fn new(
        data: Vec<u8>,
        sequence_number: i64,
        chat_id: Uuid,
        metadata: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        proof: Vec<u8>,
    ) -> Self {
        Self {
            data,
            created_at: Utc::now(),
            sequence_number,
            chat_id,
            metadata,
            payload,
            proof,
        }
    }
}
