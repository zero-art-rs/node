use axum::extract::{Path, Query};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use chrono;
use mongodb::bson;
use mongodb::bson::Uuid;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, Document};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, instrument};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

use crate::{container::Container, errors::ApiError};

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// Message content
    pub message: String,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/messages",
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
    tag = "Messages"
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
        .send_message(payload.message, &payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetMessageQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/messages/by-date/{created_at}",
    params(
        GetMessageQuery,
        ("created_at" = chrono::DateTime<chrono::Utc>, Path, description = "Message creation time")
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
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn get_message(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(mut payload): Query<GetMessageQuery>,
    Path(created_at): Path<chrono::DateTime<chrono::Utc>>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let created_at_bson = DateTime::from_millis(created_at.timestamp_millis());

    let message = state
        .messenger_service
        .get_message(&created_at_bson, &payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    match &message {
        Some(document) => info!("Found message: {}", document),
        None => info!("Message not found for date: {}", created_at_bson),
    }

    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListMessageQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Number of results to be returned
    pub limit: i64,

    /// The amount or results to skip
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/messages",
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
    tag = "Messages"
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
        .list_messages(&payload.chat_id, payload.limit, payload.skip)
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
pub struct ListCursorQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Number of results to be returned
    pub limit: i64,

    /// The amount or results to skip
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/cursors",
    params(
        ListCursorQuery
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
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn list_cursors(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<ListCursorQuery>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let cursors = state
        .messenger_service
        .list_cursors(&payload.chat_id, payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    if cursors.is_empty() {
        info!("No cursors found");
        return Ok(StatusCode::OK);
    }

    for cursor in cursors {
        info!("Found cursor record: {}", cursor);
    }

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/messages/by-date/{created_at}",
    params(
        DeleteMessageQuery,
        ("created_at" = chrono::DateTime<chrono::Utc>, Path, description = "Message creation time")
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
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn delete_message(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<DeleteMessageQuery>,
    Path(created_at): Path<chrono::DateTime<chrono::Utc>>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let created_at = DateTime::from_millis(created_at.timestamp_millis());

    let result = state
        .messenger_service
        .delete_message(&created_at, &payload.chat_id)
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
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/messages/by-id/{message_id}",
    params(
        DeleteMessageByIdQuery,
        ("message_id" = String, Path, description = "Unique message id")
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
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn delete_message_by_id(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<DeleteMessageByIdQuery>,
    Path(message_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let result = state
        .messenger_service
        .delete_message_by_id(&message_id, &payload.chat_id)
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

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct MarkAsRead {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[utoipa::path(
    put,
    path = "/v1/messenger/messages/read/{user_id}/{sequence_number}",
    params(
        MarkAsRead,
        ("user_id" = String, Path, description = "Unique user id"),
        ("sequence_number" = i64, Path, description = "Message sequence_number to be set for the user")
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
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn mark_as_read(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<MarkAsRead>,
    Path((user_id, sequence_number)): Path<(String, i64)>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let result = state
        .messenger_service
        .mark_as_read(&user_id, sequence_number, &payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    match result {
        Some(result) => {
            info!("Successfully read. The previous cursor was: {}", result);
            Ok(StatusCode::OK)
        }
        None => Ok(StatusCode::NO_CONTENT),
    }
}
