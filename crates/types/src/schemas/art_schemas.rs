use ark_ec::AffineRepr;
use ark_ff::PrimeField;
use ark_serialize::CanonicalSerialize;
use art::errors::ARTError;
use art::types::PublicART;
use cortado::CortadoAffine;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use sha3::{Digest, Sha3_256};
use utoipa::{IntoParams, IntoResponses, ToSchema};
use uuid::Uuid;
use validator::Validate;
use crate::{ARTChangesOutboxRecord, ARTChangesRecord, ARTRecord, default_limit, default_skip};

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitGroupRequest {
    /// Serialized art structure for new group.
    #[schema(value_type = Option<String>, content_encoding = "base64")]
    #[serde_as(as = "Option<Base64>")]
    pub art: Option<Vec<u8>>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Indicates whether the chat is private (one to one).
    #[schema(example = false)]
    pub is_private: bool,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetARTQuery {
    /// Serialized proof.
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,

    /// Server given challenge
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub challenge: Vec<u8>,

    /// Users invite_public_key
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub public_key: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetARTResponse {
    /// Serialized art structure
    #[schema(value_type = Option<String>, content_encoding = "base64")]
    #[serde_as(as = "Base64")]
    pub art: Vec<u8>,

    /// Defines whether the group is private or not
    pub is_private: bool,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GroupOperationRequest {
    /// Serialized BranchChanges:UpdateKeys structure
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,

    /// Additional data
    #[param(value_type = String)]
    #[serde_as(as = "Option<Base64>")]
    pub payload: Option<Vec<u8>>,
    // /// Epoch of the updated art. Currently, isn't used
    // epoch: i64
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetChangesQuery {
    /// Number of results to be returned.
    #[param(default = default_limit)]
    #[serde(default = "default_limit")]
    pub limit: i64,

    /// The amount or results to skip at first.
    #[param(default = default_skip)]
    #[serde(default = "default_skip")]
    pub skip: i64,

    /// Serialized proof.
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,

    /// The epoch of the first requested ART changes
    pub epoch: Option<i64>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CountChangesQuery {
    /// Number of results to be returned.
    #[param(default = default_limit)]
    #[serde(default = "default_limit")]
    pub limit: i64,

    /// The amount or results to skip at first.
    #[param(default = default_skip)]
    #[serde(default = "default_skip")]
    pub skip: i64,

    /// Serialized proof.
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,

    /// The epoch of the first requested ART changes
    pub epoch: Option<i64>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteGroupQuery {
    /// Serialized proof.
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,
}

#[derive(Serialize, ToSchema, Deserialize)]
pub struct ChallengeResponse {
    /// Server provided challenge
    #[schema(value_type = String, content_encoding = "base64")]
    pub challenge: Vec<u8>,
}

impl GroupOperationRequest {
    pub fn get_aux_data(&self) -> Vec<u8> {
        let mut hasher = Sha3_256::new();
        hasher.update(&self.branch_changes);
        if let Some(payload) = &self.payload {
            hasher.update(payload);
        }

        hasher.finalize()[..].to_vec()
    }
}

impl<G> TryFrom<ARTRecord<G>> for GetARTResponse
where
    G: AffineRepr,
    G::BaseField: PrimeField,
{
    type Error = ARTError;

    fn try_from(record: ARTRecord<G>) -> Result<Self, Self::Error> {
        Ok(Self{
            art: record.art .serialize()?,
            is_private: record.is_private,
        })
    }
}

