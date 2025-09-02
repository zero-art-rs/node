use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use uuid::Uuid;

#[serde_as]
#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct AuthRequest {
    #[serde_as(as = "Base64")]
    pub proof: Vec<u8>,
    #[serde_as(as = "Base64")]
    pub challenge: Vec<u8>,
    #[serde_as(as = "Base64")]
    pub nonce: Vec<u8>,
    pub chat_ids: Vec<Uuid>,
    pub epochs: Vec<i64>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AuthResponse {
    pub token: String,
}
