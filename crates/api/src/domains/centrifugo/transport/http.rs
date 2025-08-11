use std::sync::Arc;

use crate::{container::Container, errors::ApiError};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use tracing::info;
use types::utils::as_base64;

use crate::container::Container;
use types::errors::ApiError;
use uuid::Uuid;

#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct AuthRequest {
    #[serde(with = "as_base64")]
    pub public_key: Vec<u8>,
    #[serde(with = "as_base64")]
    pub proof: Vec<u8>,
    #[schema(example = r#"["3fa85f64-5717-4562-b3fc-2c963f66afa6"]"#)]
    pub chat_ids: Vec<Uuid>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AuthResponse {
    pub token: String,
}

#[utoipa::path(
    post,
    path = "/centrifugo/auth",
    tag = "Centrifugo",
    request_body = AuthRequest,
    responses(
        (status = 200, description = "Authentication successful", body = AuthResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Invalid public key"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn authenticate(
    State(container): State<Arc<Container>>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    info!("Centrifugo auth request received");
    let centrifugo_service = &container.centrifugo_service;

    info!("Generate new token..");
    let token = centrifugo_service.generate_token(&request).await?;
    info!("Token generated successfully");

    let response = AuthResponse { token };

    Ok(Json(response))
}
