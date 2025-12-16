use crate::container::Container;
use axum::{Json, extract::State};
use std::sync::Arc;
use tracing::{debug, instrument};
use types::centrifugo_schemas::{AuthRequest, AuthResponse};
use types::errors::ApiError;

/// Endpoint for receiving centrifugo subscription jvt token
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
#[instrument(skip(container, request), err)]
pub async fn authenticate(
    State(container): State<Arc<Container>>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    debug!("Centrifugo auth request received");
    let centrifugo_service = &container.centrifugo_service;

    debug!("Generate new token..");
    let token = centrifugo_service.generate_token(&request).await?;
    debug!("Token generated successfully");

    let response = AuthResponse { token };

    Ok(Json(response))
}
