use crate::utils::as_base64;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// Message content
    #[serde(with = "as_base64")]
    #[schema(example = "RXhhbXBsZSBub25jZQ==")]
    pub message: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = "3fa85f64-5717-4562-b3fc-2c963f66afa6")]
    pub chat_id: Uuid,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    #[schema(example = "RXhhbXBsZSBub25jZQ==")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde(with = "as_base64")]
    #[schema(example = "RXhhbXBsZSBub25jZQ==")]
    pub nonce: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetMessageQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = "3fa85f64-5717-4562-b3fc-2c963f66afa6")]
    pub chat_id: Uuid,

    /// Message creation time
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,

    // Unique sequence number of the message
    pub message_sequence_number: Option<i64>,

    /// Number of results to be returned
    #[param(example = 10)]
    pub limit: i64,

    /// The amount or results to skip
    #[param(example = 0)]
    pub skip: i64,

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
pub struct DeleteMessageQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = "3fa85f64-5717-4562-b3fc-2c963f66afa6")]
    pub chat_id: Uuid,

    /// Message creation time
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,

    // Unique sequence number of the message
    pub sequence_number: Option<i64>,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde(with = "as_base64")]
    pub nonce: Vec<u8>,
}
