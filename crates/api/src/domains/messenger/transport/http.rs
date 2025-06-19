use axum::extract::{Path, Query};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono;
use mongodb::bson;
use mongodb::bson::{doc, Uuid, Binary, spec::BinarySubtype};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime, Document};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use axum::response::Response;
use serde_json::json;
use tracing::{debug, info, instrument};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;
use types::Message;
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

    /// Message creation time
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,

    // Content of the message as bytes
    pub content: Option<String>,

    // Unique sequence number of the message
    pub sequence_number: Option<i64>,

    /// Number of results to be returned
    pub limit: i64,

    /// The amount or results to skip
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/messages",
    params(
        GetMessageQuery,
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
    Query(mut payload): Query<GetMessageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut filter = doc! {};

    if let Some(creation_time) = payload.created_at {
        _ = filter.insert("created_at", DateTime::from_millis(creation_time.timestamp_millis()));
    }

    if let Some(content) = payload.content {
        _ = filter.insert("content", Binary {
            subtype: BinarySubtype::Generic,
            bytes: content.into_bytes(),
        });
    }

    if let Some(sequence_number) = payload.sequence_number {
        _ = filter.insert("sequence_number", sequence_number);
    }

    let messages = state
        .messenger_service
        .list_messages(&payload.chat_id, filter.clone(), payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    if messages.is_empty() {
        info!("No messages found for filter {}", filter);
    } else {
        info!("Found next messages for the filter {}", filter);
        for message in &messages {
            info!("Found message: {}", message);
        }
    }

    let response = (
        StatusCode::OK,
        Json(messages)
    );

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Message creation time
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,

    // Content of the message as bytes
    pub content: Option<String>,

    // Unique sequence number of the message
    pub sequence_number: Option<i64>,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/messages",
    params(
        DeleteMessageQuery,
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
pub async fn delete_messages(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<DeleteMessageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut filter = doc! {};

    if let Some(creation_time) = payload.created_at {
        _ = filter.insert("created_at", DateTime::from_millis(creation_time.timestamp_millis()));
    }

    if let Some(content) = payload.content {
        _ = filter.insert("content", Binary {
            subtype: BinarySubtype::Generic,
            bytes: content.into_bytes(),
        });
    }

    if let Some(sequence_number) = payload.sequence_number {
        _ = filter.insert("sequence_number", sequence_number);
    }

    let mut removed_messages = state
        .messenger_service
        .delete_messages(&payload.chat_id, filter.clone())
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let mut status_code = StatusCode::OK;
    if removed_messages.is_empty() {
        info!("No messages found for filter {}", filter);
        status_code = StatusCode::NO_CONTENT;
    } else {
        info!("Found and removed next messages for the filter {}", filter);
        for message in &removed_messages {
            info!("Successfully deleted message: {}", message);
        }
    }

    let response = (
        status_code,
        Json(removed_messages)
    );

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct MarkAsRead {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Unique user id
    pub user_id: String,

    /// Message sequence_number to be set for the user
    pub sequence_number: i64,
}

#[utoipa::path(
    put,
    path = "/v1/messenger/cursors",
    params(
        MarkAsRead,
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
) -> Result<impl IntoResponse, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let result = state
        .messenger_service
        .mark_as_read(&payload.user_id, payload.sequence_number, &payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let response;
    match result {
        Some(result) => {
            info!("Successfully read. The previous cursor was: {}", result);
            response = (
                StatusCode::OK,
                Json(Some(result))
            );
        }
        None => response = (
            StatusCode::NO_CONTENT,
            Json(None)
        )
    }

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetCursorsQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// User unique identifier
    pub user_id: Option<String>,

    /// Sequence number of the message, which user already read
    pub cursor: Option<i64>,

    /// Number of results to be returned
    pub limit: i64,

    /// The amount or results to skip
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/cursors",
    params(
        GetCursorsQuery,
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
    Query(mut payload): Query<GetCursorsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut filter = doc! {};

    if let Some(user_id) = payload.user_id {
        _ = filter.insert("user_id", user_id);
    }

    if let Some(cursor) = payload.cursor {
        _ = filter.insert("cursor", cursor);
    }

    let cursors = state
        .messenger_service
        .list_cursors(&payload.chat_id, filter.clone(), payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    if cursors.is_empty() {
        info!("No cursors found for filter {}", filter);
    } else {
        info!("Found next cursors for the filter {}", filter);
        for cursor in &cursors {
            info!("Found cursor: {}", cursor);
        }
    }

    let response = (
        StatusCode::ACCEPTED,
        Json(cursors)
    );

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCursorsQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// User unique identifier
    pub user_id: Option<String>,

    /// Sequence number of the message, which user already read
    pub cursor: Option<i64>,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/cursors",
    params(
        DeleteCursorsQuery,
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
pub async fn delete_cursors(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<DeleteCursorsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut filter = doc! {};

    if let Some(user_id) = payload.user_id {
        _ = filter.insert("user_id", user_id);
    }

    if let Some(cursor) = payload.cursor {
        _ = filter.insert("cursor", cursor);
    }

    let mut removed_cursors = state
        .messenger_service
        .delete_cursors(&payload.chat_id, filter.clone())
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let mut status_code = StatusCode::OK;
    if removed_cursors.is_empty() {
        info!("No cursors found for filter {}", filter);
        status_code = StatusCode::NO_CONTENT;
    } else {
        info!("Found and removed cursors for the filter {}:", filter);
        for message in &removed_cursors {
            info!("Successfully deleted cursor: {}", message);
        }
    }

    let response = (
        status_code,
        Json(removed_cursors)
    );

    Ok(response)
}
