use crate::container::Container;
use crate::domains::art::transport::utils::{decode_art, decode_branch_changes};
use art::types::{ARTNode, BranchChangesType, PublicART};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use mongodb::bson::doc;
use std::sync::Arc;
use cortado::CortadoAffine;
use tracing::{debug, error, instrument};
use types::{
    art_schemas::*,
    errors::{ARTServiceError, ApiError},
};
use uuid::Uuid;
use validator::Validate;

const DEFAULT_CHALLENGE_LENGTH: u32 = 16; // 16 bytes

/// Create a new group
#[utoipa::path(
    post,
    path = "/v1/group",
    request_body = InitGroupRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn init_chat(
    State(state): State<Arc<Container>>,
    Json(payload): Json<InitGroupRequest>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    match &payload.art {
        Some(art) => {
            state
                .art_service
                .init_chat(
                    &payload.chat_id,
                    decode_art(&art)?,
                    payload.is_private,
                )
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
        }
        None => {
            #[cfg(not(feature = "art_modifications"))]{
                debug!("Create group with empty art");
                state
                    .art_service
                    .init_chat(
                        &payload.chat_id,
                        PublicART {
                            root: Box::new(ARTNode::new_leaf(CortadoAffine::default())),
                            generator: CortadoAffine::default(),
                        },
                        payload.is_private,
                    ).await.map_err(|e| ApiError::InternalServerError(e.to_string()))?;
            }
            #[cfg(feature = "art_modifications")]{
                error!("ART as None is not supported");
                return Err(ApiError::from(ARTServiceError::InvalidInput))
            }
        }
    }


    debug!(
        "Successfully created new chat with id: {}",
        &payload.chat_id
    );

    Ok(StatusCode::CREATED)
}

/// Get ART structure
#[utoipa::path(
    get,
    path = "/v1/group/{id}/{epoch}",
    params(
        GetARTQuery,
        ("id" = Uuid, Path, description = "Group id"),
        ("epoch" = i64, Path, description = "Get art at provided epoch")
    ),
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn get_art(
    State(state): State<Arc<Container>>,
    Path((chat_id, epoch)): Path<(Uuid, i64)>,
    Query(payload): Query<GetARTQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    let art_record = state
        .art_service
        .get_art(&chat_id, Some(epoch))
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let art_bytes = art_record
        .art
        .serialize()
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok((StatusCode::OK, BASE64_STANDARD.encode(art_bytes)))
}

/// Send ART update (key update, add member, remove member)
#[utoipa::path(
    put,
    path = "/v1/group/{id}",
    params(
        GetARTQuery,
        ("id" = Uuid, Path, description = "Group id")
    ),
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn update_art(
    State(state): State<Arc<Container>>,
    Path(chat_id): Path<Uuid>,
    Json(payload): Json<GroupOperationRequest>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    state.start_updating(chat_id).await?;

    let branch_changes = match decode_branch_changes(&payload.branch_changes) {
        Ok(branch_changes) => branch_changes,
        Err(e) => {
            state.stop_updating(chat_id).await;
            return Err(e);
        }
    };

    if let Err(e) = state
        .art_service
        .update_art(&chat_id, &branch_changes, payload.payload, &payload.proof)
        .await
    {
        state.stop_updating(chat_id).await;
        return Err(e.into());
    }

    state.stop_updating(chat_id).await;

    match branch_changes.change_type {
        BranchChangesType::UpdateKey => Ok(StatusCode::OK),
        BranchChangesType::AppendNode => Ok(StatusCode::OK),
        BranchChangesType::MakeBlank => Ok(StatusCode::NO_CONTENT),
        _ => Ok(StatusCode::NOT_IMPLEMENTED),
    }
}

/// Get ART changes
#[utoipa::path(
    get,
    path = "/v1/group/{id}",
    params(
        GetChangesQuery,
        ("id" = Uuid, Path, description = "Group id")
    ),
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn get_changes(
    State(state): State<Arc<Container>>,
    Path(chat_id): Path<Uuid>,
    Query(payload): Query<GetChangesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    let filter = doc! { "epoch": { "$gte": payload.epoch } };
    let changes = state
        .art_service
        .list_changes(&chat_id, filter.clone(), payload.limit, payload.skip)
        .await?;

    debug!(
        "Found {} changes for filter: {}, skip: {} and limit: {}",
        changes.len(),
        filter,
        payload.skip,
        payload.limit
    );

    Ok((
        StatusCode::OK,
        postcard::to_allocvec(&changes)
            .map_err(|e| ApiError::InternalServerError(e.to_string()))?,
    ))
}

/// Delete the group
#[utoipa::path(
    delete,
    path = "/v1/group/{id}",
    params(
        DeleteGroupQuery,
        ("id" = Uuid, Path, description = "Group id")
    ),
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn delete_chat(
    State(state): State<Arc<Container>>,
    Path(chat_id): Path<Uuid>,
    Query(payload): Query<DeleteGroupQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    debug!("Delete chat: {}", chat_id);
    state
        .art_service
        .delete_chat(&chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    state.art_is_updating.write().await.remove(&chat_id);
    debug!("Deletion is successful");

    Ok(StatusCode::NO_CONTENT)
}

/// Get challenge from the server
#[utoipa::path(
    get,
    path = "/v1/group/{id}/challenge",
    params(("id" = Uuid, Path, description = "Group id")),
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn get_challenge(
    State(state): State<Arc<Container>>,
) -> Result<impl IntoResponse, ApiError> {
    debug!("Create a write lock on challenges");
    let mut lock = state.challenges.write().await;
    debug!("Create new challenge");
    let challenge = (0..DEFAULT_CHALLENGE_LENGTH)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();
    lock.insert(challenge.clone());

    Ok((StatusCode::OK, BASE64_STANDARD.encode(challenge)))
}
