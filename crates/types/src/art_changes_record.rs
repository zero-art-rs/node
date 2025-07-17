use ark_ec::AffineRepr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::types::BranchChanges;
use bson::DateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound = "")]
pub struct ARTChangesRecord<G>
where
    G: AffineRepr + CanonicalSerialize + CanonicalDeserialize,
{
    /// ART changes
    pub changes: BranchChanges<G>, //BranchChanges<G>,
    /// When the message was created
    pub created_at: DateTime,
    /// Sequential number of this message in the chat
    pub sequence_number: i64,
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ARTChangesOutboxRecord {
    /// ART changes
    pub data: Vec<u8>,
    /// When the message was created
    pub created_at: DateTime,
    /// Sequential number of this message in the chat
    pub sequence_number: i64,
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

impl<G> ARTChangesRecord<G>
where
    G: AffineRepr + CanonicalSerialize + CanonicalDeserialize,
{
    pub fn new(data: BranchChanges<G>, sequence_number: i64, chat_id: Uuid) -> Self {
        Self {
            changes: data,
            created_at: DateTime::now(),
            sequence_number,
            chat_id,
        }
    }
}

impl ARTChangesOutboxRecord {
    pub fn new(data: Vec<u8>, sequence_number: i64, chat_id: Uuid) -> Self {
        Self {
            data,
            created_at: DateTime::now(),
            sequence_number,
            chat_id,
        }
    }
}
