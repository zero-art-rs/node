use crate::utils::as_base64;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitChatRequest {
    /// Serialized art structure for new chat.
    #[serde(with = "as_base64")]
    pub art: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Indicates whether the chat is private (one to one).
    pub is_private: bool,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetARTQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde(with = "as_base64")]
    pub nonce: Vec<u8>,

    /// Sequence number of the requested art. If not set, return the latest.
    pub sequence_number: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetInitialARTQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub signature: Vec<u8>,
    
    /// User provided nonce
    #[serde(with = "as_base64")]
    pub nonce: Vec<u8>,

    /// Server given challenge
    #[serde(with = "as_base64")]
    pub challenge: Vec<u8>,

    /// Users invite_public_key
    #[serde(with = "as_base64")]
    pub public_key: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberRequest {
    /// Serialized BranchChanges:AppendNode structure.
    #[serde(with = "as_base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub proof: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMemberRequest {
    /// Serialized BranchChanges:UpdateKeys structure.
    #[serde(with = "as_base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub proof: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateKeyRequest {
    /// Serialized BranchChanges:UpdateKeys structure
    #[serde(with = "as_base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof
    #[serde(with = "as_base64")]
    pub proof: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetChangesQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Number of results to be returned.
    pub limit: i64,

    /// The amount or results to skip at first.
    pub skip: i64,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub signature: Vec<u8>,

    /// User's leaf node index
    pub index: u32,

    /// User provided nonce
    #[serde(with = "as_base64")]
    pub nonce: Vec<u8>,

    /// Sequence number of the requested art. If not set, return the latest.
    pub sequence_number: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteChatQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde(with = "as_base64")]
    pub nonce: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMetadataRequest {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Users node index
    pub index: u32,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde(with = "as_base64")]
    pub nonce: Vec<u8>,

    /// New metadata
    #[serde(with = "as_base64")]
    pub metadata: Vec<u8>,
}
