use crate::container::Container;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum_core::body::Body;
use axum_core::extract::{FromRequest, FromRequestParts, Request};
use axum_core::response::Response;
use mongodb::bson::{DateTime, doc};
use std::sync::Arc;
use tracing::{debug, error, instrument};
use types::errors::ApiError;
use types::errors::VerificationError;
use types::messenger_schemas::{GetMessageQuery, SendMessageRequest};
use uuid::Uuid;
use validator::Validate;

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
    Path(path): Path<Uuid>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    state
        .messenger_service
        .send_message(payload.message, &path, payload.epoch)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::ACCEPTED)
}

#[utoipa::path(
    get,
    path = "/v1/group/{id}/messages",
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
    Path(chat_id): Path<Uuid>,
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
        .list_messages(&chat_id, filter.clone(), payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    if messages.is_empty() {
        debug!("No messages found for filter {}", filter);
    } else {
        debug!("Found next messages for the filter {}", filter);
        for message in &messages {
            debug!("Found message: {}", message);
        }
    }

    Ok((StatusCode::OK, Json(messages)))
}
