use crate::container::Container;
use axum::{
    Json,
    extract::{Path, Query, State},
};
use mongodb::bson::doc;
use std::sync::Arc;
use tracing::instrument;
use types::{
    art_schemas::{ChallengeResponse, GetARTQuery, GetARTResponse},
    errors::ApiError,
};
use uuid::Uuid;
use validator::Validate;

/// Get ART structure
#[utoipa::path(
    get,
    path = "/v1/group/{id}/{epoch}",
    params(
        GetARTQuery,
        ("id" = Uuid, Path, description = "Group id"),
        ("epoch" = u64, Path, description = "Get art at provided epoch")
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
    Path((chat_id, epoch)): Path<(Uuid, u64)>,
    Query(payload): Query<GetARTQuery>,
) -> Result<Json<GetARTResponse>, ApiError> {
    payload.validate()?;

    let mut art_record = state.art_service.get_art(chat_id, Some(epoch)).await?;
    art_record.art.commit()?;
    
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
    // let challenge = state.new_challenge().await;

    Ok(Json(ChallengeResponse {
        challenge: state.new_challenge().await,
    }))
}
