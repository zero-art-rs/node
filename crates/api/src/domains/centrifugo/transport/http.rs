use std::sync::Arc;

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::{container::Container, errors::ApiError};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AuthRequest {
    #[schema(example = r#"[1]"#)]
    pub public_key: Vec<u8>,
    #[schema(example = r#"[1]"#)]
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
    let centrifugo_service = &container.centrifugo_service;

    let token = centrifugo_service.generate_token(&request).await?;

    let response = AuthResponse { token };

    Ok(Json(response))
}
