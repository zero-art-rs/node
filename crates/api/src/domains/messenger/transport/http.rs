use crate::container::Container;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use mongodb::bson::doc;
use std::sync::Arc;
use tracing::{debug, instrument};
use types::MessageRecord;
use types::errors::{ApiError, MessageServiceError};
use types::messenger_schemas::{CountMessagesQuery, GetMessageQuery, SendMessageRequest};
use uuid::Uuid;
use validator::Validate;

/// Endpoint for requesting messages from the group
#[utoipa::path(
    get,
    path = "/v1/group/{id}",
    params(
        GetMessageQuery,
    ),
    responses(
        (status = 202, description = "Successfully retrieved messages.", body = Vec<MessageRecord>),
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
) -> Result<Json<Vec<MessageRecord>>, ApiError> {
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
    path = "/v1/group/{id}/count",
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
