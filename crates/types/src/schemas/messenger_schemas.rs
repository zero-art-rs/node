use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;
use crate::{default_limit, default_skip};

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// Message content
    #[schema(value_type = String, content_encoding = "base64")]
    #[serde_as(as = "Base64")]
    pub message: Vec<u8>,

    /// Sequential number of epochs during which the message was sent
    pub epoch: i64,

    /// Serialized proof.
    #[schema(value_type = String, content_encoding = "base64")]
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[schema(value_type = String, content_encoding = "base64")]
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetMessageQuery {
    // Unique sequence number of the message. Default is 0.
    pub message_sequence_number: Option<i64>,

    /// Number of results to be returned
    #[param(default = default_limit)]
    #[serde(default = "default_limit")]
    pub limit: i64,

    /// The amount or results to skip
    #[param(default = default_skip)]
    #[serde(default = "default_skip")]
    pub skip: i64,

    /// Serialized signature.
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,

    /// Sequence number of the art used in proof. Default is 0.
    pub epoch: Option<i64>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CountMessagesQuery {
    // Unique sequence number of the message. Default is 0.
    pub message_sequence_number: Option<i64>,

    /// Number of results to be returned
    #[param(default = default_limit)]
    #[serde(default = "default_limit")]
    pub limit: i64,

    /// The amount or results to skip
    #[param(default = default_skip)]
    #[serde(default = "default_skip")]
    pub skip: i64,

    /// Serialized signature.
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[param(value_type = String)]
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,

    /// Sequence number of the art used in proof. Default is 0.
    pub epoch: Option<i64>,
}
