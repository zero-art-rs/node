use crate::container::Container;
use crate::verification_middleware;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use bytes::{Bytes, BytesMut};
use mongodb::bson::doc;
use std::sync::Arc;
use tracing::instrument;
use types::errors::ApiError;
use types::messenger_schemas::{CountMessagesQuery, GetMessageQuery};
use types::protos::{Frame, SpFrames};
use uuid::Uuid;
use validator::Validate;

/// Send Frame to the group
#[utoipa::path(
    post,
    path = "/v1/group/{id}/frames",
    request_body(
        content = Frame,
        content_type = "application/protobuf",
        description = "Frame encoded with protobuf"
    ),
    params(
        ("id" = Uuid, Path, description = "Group id"),
    ),
    responses(
        (status = 200, description = "Authentication successful"),
    ),
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn send_frame(
    State(state): State<Arc<Container>>,
    Path(id): Path<Uuid>,
    body: Bytes,
) -> Result<StatusCode, ApiError> {
    state.send_frame(id, body).await.map_err(ApiError::from)
}

/// Endpoint for requesting messages from the group
#[utoipa::path(
    get,
    path = "/v1/group/{id}/frames",
    params(
        GetMessageQuery,
        ("id" = Uuid, Path, description = "Group id"),
    ),
    responses(
        (status = 202, description = "Successfully retrieved messages.", body = SpFrames, content_type = "application/protobuf"),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn list_messages(
    State(state): State<Arc<Container>>,
    Path(id): Path<Uuid>,
    Query(payload): Query<GetMessageQuery>,
) -> Result<(StatusCode, BytesMut), ApiError> {
    payload.validate()?;

    let mut filter = doc! {};

    if let Some(sequence_number) = payload.message_sequence_number {
        filter.insert("sequence_number", doc! { "$gte": sequence_number });
    }

    if let Some(epoch) = payload.epoch {
        filter.insert("epoch", doc! { "$gte": epoch as i64 });
    }

    let messages = state
        .messenger_service
        .list_messages(&id, filter.clone(), payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok((StatusCode::ACCEPTED, messages))
}

/// Endpoint counting messages
#[utoipa::path(
    get,
    path = "/v1/group/{id}/frames/count",
    params(
        CountMessagesQuery,
        ("id" = Uuid, Path, description = "Group id"),
    ),
    responses(
        (status = 202, description = "Successfully counted messages.", body = u64),
    ),
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn count_messages(
    State(state): State<Arc<Container>>,
    Path(id): Path<Uuid>,
    Query(payload): Query<CountMessagesQuery>,
) -> Result<(StatusCode, Json<u64>), ApiError> {
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
        .count_messages(id, filter.clone(), payload.limit, payload.skip)
        .await?;

    Ok((StatusCode::ACCEPTED, Json(count)))
}
