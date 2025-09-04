use crate::container::Container;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use mongodb::bson::{doc};
use std::sync::Arc;
use tracing::{debug, instrument};
use types::errors::{ApiError, MessengerError};
use types::messenger_schemas::{GetMessageQuery, CountMessagesQuery, SendMessageRequest};
use uuid::Uuid;
use validator::Validate;
use types::Message;

/// Endpoint for sending message to the group
#[utoipa::path(
    post,
    path = "/v1/group/{id}/messages",
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
    Path(chat_id): Path<Uuid>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    state.art_service.get_initial_art(&chat_id)
        .await
        .map_err(|_| MessengerError::GroupNotExists)?;

    state
        .messenger_service
        .send_message(payload.message, &chat_id, payload.epoch)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::ACCEPTED)
}

/// Endpoint for requesting messages from the group
#[utoipa::path(
    get,
    path = "/v1/group/{id}/messages",
    params(
        GetMessageQuery,
    ),
    responses(
        (status = 202, description = "Successfully retrieved messages.", body = Vec<Message>),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn list_messages(
    State(state): State<Arc<Container>>,
    Path(chat_id): Path<Uuid>,
    Query(payload): Query<GetMessageQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    payload.validate()?;

    let mut filter = doc! {};

    if let Some(sequence_number) = payload.message_sequence_number {
        filter.insert("sequence_number", doc! { "$gte": sequence_number });
    }

    if let Some(epoch) = payload.epoch {
        filter.insert("epoch", doc! { "$gte": epoch });
    }

    let messages = state
        .messenger_service
        .list_messages(&chat_id, filter.clone(), payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    if messages.is_empty() {
        debug!("No messages found for filter {}", &filter);
    } else {
        debug!("Found next messages for the filter {}", &filter);
        for message in &messages {
            debug!("Found message: {}", message);
        }
    }

    Ok(Json(messages))
}

/// Endpoint counting messages
#[utoipa::path(
    get,
    path = "/v1/group/{id}/messages/count",
    params(
        CountMessagesQuery,
    ),
    responses(
        (status = 200, description = "Successfully counted messages.", body = u64),
    ),
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn count_messages(
    State(state): State<Arc<Container>>,
    Path(chat_id): Path<Uuid>,
    Query(payload): Query<CountMessagesQuery>,
) -> Result<Json<u64>, ApiError> {
    payload.validate()?;

    let mut filter = doc! {};

    if let Some(sequence_number) = payload.message_sequence_number {
        filter.insert("sequence_number", doc! { "$gte": sequence_number });
    }

    if let Some(epoch) = payload.epoch {
        filter.insert("epoch", doc! { "$gte": epoch });
    }

    let count = state
        .messenger_service
        .count_messages(&chat_id, filter.clone(), payload.limit, payload.skip)
        .await?;

    Ok(Json(count))
}
