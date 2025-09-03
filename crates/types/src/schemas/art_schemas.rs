use std::ptr::hash;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;
use sha3::{Digest, Sha3_256};

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitChatRequest {
    /// Serialized art structure for new chat.
    #[serde_as(as = "Base64")]
    pub art: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Indicates whether the chat is private (one to one).
    pub is_private: bool,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetARTQuery {
    /// Serialized proof.
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,

    /// Server given challenge
    #[serde_as(as = "Base64")]
    pub challenge: Vec<u8>,

    /// Users invite_public_key
    #[serde_as(as = "Base64")]
    pub public_key: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GroupOperationRequest {
    /// Serialized BranchChanges:UpdateKeys structure
    #[serde_as(as = "Base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,

    /// Additional data
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
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,

    /// The epoch of the first requested ART changes
    pub epoch: i64,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteChatQuery {
    /// Serialized proof.
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
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
