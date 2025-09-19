use crate::{default_limit, default_skip};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_with::{base64::{Base64, UrlSafe}, serde_as};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetMessageQuery {
    // Unique sequence number of the first message. Default is 0.
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
    #[serde_as(as = "Base64<UrlSafe>")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[param(value_type = String)]
    #[serde_as(as = "Base64<UrlSafe>")]
    pub nonce: Vec<u8>,

    /// Sequence number of the art used in proof and the . Default is 0.
    pub epoch: Option<u64>,
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
    #[serde_as(as = "Base64<UrlSafe>")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[param(value_type = String)]
    #[serde_as(as = "Base64<UrlSafe>")]
    pub nonce: Vec<u8>,

    /// Sequence number of the art used in proof. Default is 0.
    pub epoch: Option<i64>,
}
