use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::instrument;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use crate::{container::Container, errors::ApiError};

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// Hex-encoded public key of the user sending the message.
    #[validate(length(equal = 33))]
    #[schema(example = "03abcdef0123456789abcdef0123456789abcdef0123456789abcdef01234567")]
    pub user_public_key: String,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/send",
    request_body = SendMessageRequest,
    responses(
        (status = 202, description = "Message sent."),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "Liquidity"
)]
#[instrument(skip(state, headers), err)]
pub async fn send_message(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<SendMessageRequest>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    Ok(StatusCode::ACCEPTED)
}
