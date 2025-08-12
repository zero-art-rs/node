use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// Message content
    #[serde_as(as = "Base64")]
    pub message: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Sequential number of epoch during which the message was sent
    pub epoch: u32,

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
pub struct GetMessageQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Message creation time
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,

    // Unique sequence number of the message
    pub message_sequence_number: Option<u32>,

    /// Number of results to be returned
    pub limit: u32,

    /// The amount or results to skip
    pub skip: u32,

    /// Serialized proof.
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,

    /// Sequence number of the art used in proof. If not set, return the latest.
    pub epoch: Option<u32>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Message creation time
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,

    // Unique sequence number of the message
    pub sequence_number: Option<u32>,

    /// Serialized proof.
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,
}
