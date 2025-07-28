use crate::container::Container;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use mongodb::bson::{DateTime, doc};
use std::sync::Arc;
use tracing::{info, instrument};
use types::errors::ApiError;
use validator::Validate;

use types::messenger_schemas::{DeleteMessageQuery, GetMessageQuery, SendMessageRequest};

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
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn send_message(
    State(state): State<Arc<Container>>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<StatusCode, ApiError> {
    payload.validate()?;

    state
        .messenger_service
        .send_message(payload.message, &payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::ACCEPTED)
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
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn list_messages(
    State(state): State<Arc<Container>>,
    Query(payload): Query<GetMessageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    let mut filter = doc! {};

    if let Some(creation_time) = payload.created_at {
        filter.insert(
            "created_at",
            DateTime::from_millis(creation_time.timestamp_millis()),
        );
    }

    if let Some(sequence_number) = payload.message_sequence_number {
        filter.insert("sequence_number", sequence_number);
    }

    let messages = state
        .messenger_service
        .list_messages(
            &payload.chat_id,
            filter.clone(),
            payload.limit,
            payload.skip,
        )
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

    let response = (StatusCode::OK, Json(messages));

    Ok(response)
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
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn delete_messages(
    State(state): State<Arc<Container>>,
    Query(payload): Query<DeleteMessageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    let mut filter = doc! {};

    if let Some(creation_time) = payload.created_at {
        filter.insert(
            "created_at",
            DateTime::from_millis(creation_time.timestamp_millis()),
        );
    }

    if let Some(sequence_number) = payload.sequence_number {
        filter.insert("sequence_number", sequence_number);
    }

    let removed_messages = state
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

    let response = (status_code, Json(removed_messages));

    Ok(response)
}
