use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

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
pub struct AddMemberRequest {
    /// Serialized BranchChanges:AppendNode structure.
    #[serde_as(as = "Base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof.
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMemberRequest {
    /// Serialized BranchChanges:UpdateKeys structure.
    #[serde_as(as = "Base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof.
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateKeyRequest {
    /// Serialized BranchChanges:UpdateKeys structure
    #[serde_as(as = "Base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,

    /// Optional new user metadata
    #[serde_as(as = "Option<Base64>")]
    pub metadata: Option<Vec<u8>>,

    /// Additional data
    #[serde_as(as = "Option<Base64>")]
    pub payload: Option<Vec<u8>>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateARTRequest {
    /// Serialized BranchChanges:UpdateKeys structure
    #[serde_as(as = "Base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,

    /// Optional new user metadata
    #[serde_as(as = "Option<Base64>")]
    pub metadata: Option<Vec<u8>>,

    /// Additional data
    #[serde_as(as = "Option<Base64>")]
    pub payload: Option<Vec<u8>>,
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
