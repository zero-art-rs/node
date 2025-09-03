use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use sha3::{Digest, Sha3_256};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitGroupRequest {
    /// Serialized art structure for new chat.
    #[schema(
        value_type = Option<String>,
        content_encoding = "base64",
    )]
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
    pub limit: i64,

    /// The amount or results to skip at first.
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
    pub epoch: i64,
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
