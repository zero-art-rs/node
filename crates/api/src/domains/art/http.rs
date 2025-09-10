use crate::container::Container;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use mongodb::bson::doc;
use std::sync::Arc;
use tracing::{debug, instrument};
use types::{art_schemas::*, errors::ApiError, protos::Frame};
use uuid::Uuid;
use validator::Validate;

const DEFAULT_CHALLENGE_LENGTH: u32 = 16; // 16 bytes

/// Get ART structure
#[utoipa::path(
    get,
    path = "/v1/group/{id}/{epoch}",
    params(
        GetARTQuery,
        ("id" = Uuid, Path, description = "Group id"),
        ("epoch" = i64, Path, description = "Get art at provided epoch")
    ),
    responses(
        (status = 200, description = "ART retrieved successfully", body = GetARTResponse),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    tag = "Group operations"
)]
#[instrument(skip(state), err)]
pub async fn get_art(
    State(state): State<Arc<Container>>,
    Path((chat_id, epoch)): Path<(Uuid, i64)>,
    Query(payload): Query<GetARTQuery>,
) -> Result<Json<GetARTResponse>, ApiError> {
    payload.validate()?;

    let art_record = state
        .art_service
        .get_art(&chat_id, Some(epoch))
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(Json(GetARTResponse::try_from(art_record)?))
}

/// Get a challenge from the server
#[utoipa::path(
    get,
    path = "/v1/group/{id}/challenge",
    params(("id" = Uuid, Path, description = "Group id")),
    responses(
        (status = 200, description = "Challenge Sent.", body = ChallengeResponse),
    ),
    tag = "Group operations"
)]
#[instrument(skip(state), err)]
pub async fn get_challenge(
    State(state): State<Arc<Container>>,
) -> Result<Json<ChallengeResponse>, ApiError> {
    debug!("Create a write lock on challenges and create new challenge...");
    let mut lock = state.challenges.write().await;

    let challenge = (0..DEFAULT_CHALLENGE_LENGTH)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();

    lock.insert(challenge.clone());
    drop(lock);
    debug!("Challenge created and lock is dropped");

    Ok(Json(ChallengeResponse { challenge }))
}
