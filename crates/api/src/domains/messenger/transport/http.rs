use axum::extract::Query;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use mongodb::bson::Uuid;
use mongodb::bson::{DateTime, Document};
use chrono;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use mongodb::bson;
use tracing::{debug, info, instrument};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

use crate::{container::Container, errors::ApiError};

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
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
    /// message creation time
    pub created_at: chrono::DateTime<chrono::Utc>,
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
    Query(mut payload): Query<GetMessageQuery>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let created_at = DateTime::from_millis(payload.created_at.timestamp_millis());

    let message = state
        .messenger_service
        .get_message(&created_at)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    match &message {
        Some(document) => info!("Found message: {}", document),
        None => info!("Message not found for date: {}", created_at),
    }

    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListMessageQuery {}

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

    let messages = state
        .messenger_service
        .list_messages()
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    if messages.is_empty() {
        info!("No messages found");
        return Ok(StatusCode::OK);
    }

    for message in messages {
        info!("Found message: {}", message);
    }

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageQuery {
    /// Message creation time
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/delete",
    params(
        DeleteMessageQuery
    ),
    responses(
        (status = 202, description = "Message sent."),
        (status = 204, description = "No Content. Removed successfully."),
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
pub async fn delete_message(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<DeleteMessageQuery>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let created_at = DateTime::from_millis(payload.created_at.timestamp_millis());

    let result = state
        .messenger_service
        .delete_message(&created_at)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    match result {
        Some(result) => {
            info!("Successfully deleted message: {}", result);
            Ok(StatusCode::OK)
        }
        None => {
            info!("No message found");
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageByIdQuery {
    /// Unique message id
    pub message_id: String,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/delete_by_id",
    params(
        DeleteMessageByIdQuery
    ),
    responses(
        (status = 202, description = "Message sent."),
        (status = 204, description = "No Content. Remove successfully."),
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
pub async fn delete_message_by_id(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<DeleteMessageByIdQuery>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let result = state
        .messenger_service
        .delete_message_by_id(&payload.message_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    match result {
        Some(result) => {
            info!("Successfully deleted message: {}", result);
            Ok(StatusCode::OK)
        }
        None => Ok(StatusCode::NO_CONTENT),
    }
}
