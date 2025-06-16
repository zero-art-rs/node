use axum::extract::Query;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, instrument};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::{container::Container, errors::ApiError};

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// Hex-encoded public key of the user sending the message.
    #[validate(length(equal = 33))]
    #[schema(example = "03abcdef0123456789abcdef012345678")]
    pub user_public_key: String,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Message content
    pub message: String,
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

    state
        .messenger_service
        .send_message(payload.message)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetMessageQuery {
    /// Hex-encoded public key of the user sending the message.
    #[validate(length(equal = 33))]
    #[schema(example = "03abcdef0123456789abcdef012345678")]
    pub user_public_key: String,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = "b3d56c7e-4c1a-4f8e-9f8a-7f9f7f9f7f9f")]
    pub chat_id: Uuid,

    /// Unique message id
    pub message_id: String,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/get",
    params(
        GetMessageQuery
    ),
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
pub async fn get_message(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<GetMessageQuery>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let message = state
        .messenger_service
        .get_message(payload.message_id.clone())
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    match &message {
        Some(document) => info!("Found message: {}", document),
        None => info!("Message not found for id: {}", payload.message_id),
    }

    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListMessageQuery {
    /// Hex-encoded public key of the user sending the message.
    #[validate(length(equal = 33))]
    #[schema(example = "03abcdef0123456789abcdef012345678")]
    pub user_public_key: String,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = "b3d56c7e-4c1a-4f8e-9f8a-7f9f7f9f7f9f")]
    pub chat_id: Uuid,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/list",
    params(
        ListMessageQuery
    ),
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
pub async fn list_messages(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<ListMessageQuery>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let message = state
        .messenger_service
        .list_messages()
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    for message in message {
        info!("Found message: {}", message);
    }

    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageQuery {
    /// Hex-encoded public key of the user sending the message.
    #[validate(length(equal = 33))]
    #[schema(example = "03abcdef0123456789abcdef012345678")]
    pub user_public_key: String,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = "b3d56c7e-4c1a-4f8e-9f8a-7f9f7f9f7f9f")]
    pub chat_id: Uuid,

    /// Unique message id
    pub message_id: String,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/delete",
    request_body = DeleteMessageQuery,
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
pub async fn delete_messages(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<DeleteMessageQuery>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state.messenger_service
        .delete_messages(payload.message_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}