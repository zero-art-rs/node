use ark_ec::AffineRepr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::errors::ARTError;
use art::types::BranchChanges;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use utoipa::ToSchema;
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
    pub epoch: i64,
    pub chat_id: Uuid,
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
pub struct ARTChangesOutboxRecord {
    /// Serialized branch changes
    #[schema(value_type = Option<String>, content_encoding = "base64")]
    #[serde_as(as = "Base64")]
    pub changes: Vec<u8>,

    /// Creation time
    pub created_at: DateTime<Utc>,

    /// Server given sequence number of record
    pub sequence_number: i64,

    /// User provided epoch of updated art
    pub epoch: i64,

    /// Id of the group
    pub chat_id: Uuid,

    /// Serialized proof
    #[schema(value_type = Option<String>, content_encoding = "base64")]
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
        epoch: i64,
        chat_id: Uuid,
        proof: Vec<u8>,
    ) -> Self {
        Self {
            changes: data,
            created_at: Utc::now(),
            sequence_number,
            epoch,
            chat_id,
            proof,
        }
    }
}

impl ARTChangesOutboxRecord {
    pub fn new(
        changes: Vec<u8>,
        sequence_number: i64,
        epoch: i64,
        chat_id: Uuid,
        proof: Vec<u8>,
    ) -> Self {
        Self {
            changes,
            created_at: Utc::now(),
            sequence_number,
            epoch,
            chat_id,
            proof,
        }
    }
}

impl<G> TryFrom<ARTChangesRecord<G>> for ARTChangesOutboxRecord
where
    G: AffineRepr,
{
    type Error = ARTError;

    fn try_from(record: ARTChangesRecord<G>) -> Result<Self, Self::Error> {
        Ok(Self {
            changes: record.changes.serialze()?,
            created_at: record.created_at,
            sequence_number: record.sequence_number,
            epoch: record.epoch,
            chat_id: record.chat_id,
            proof: record.proof,
        })
    }
}

impl<G> TryFrom<ARTChangesOutboxRecord> for ARTChangesRecord<G>
where
    G: AffineRepr,
{
    type Error = ARTError;

    fn try_from(value: ARTChangesOutboxRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            changes: BranchChanges::<G>::deserialize(value.changes.as_slice())?,
            created_at: value.created_at,
            sequence_number: value.sequence_number,
            epoch: value.epoch,
            chat_id: value.chat_id,
            proof: value.proof,
        })
    }
}
