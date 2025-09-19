use serde::{Deserialize, Serialize};
use serde_with::{base64::{Base64, UrlSafe}, serde_as};
use uuid::Uuid;

#[serde_as]
#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct AuthRequest {
    /// Server provided challenge.
    #[schema(value_type = String, content_encoding = "Base64")]
    #[serde_as(as = "Base64")]
    pub challenge: Vec<u8>,

    // User provided nonce.
    #[schema(value_type = String, content_encoding = "Base64")]
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,

    /// Set of ids to subscribe to.
    pub chat_ids: Vec<Uuid>,

    /// Epochs which are known to user for each group id. Default is 0.
    pub epochs: Vec<u64>,

    /// Proof of art root secret key knowledge for each group id.
    #[schema(value_type = String, content_encoding = "Base64")]
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AuthResponse {
    pub token: String,
}
