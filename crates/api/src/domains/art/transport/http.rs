use crate::container::Container;
use crate::domains::art::transport::utils::{decode_art, decode_branch_changes};
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use mongodb::bson::doc;
use std::sync::Arc;
use tracing::{error, info, instrument};
use types::art_schemas::*;
use types::errors::{ARTServiceError, ApiError};
use validator::Validate;

#[utoipa::path(
    post,
    path = "/v1/messenger/init-chat",
    request_body = InitChatRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn init_chat(
    State(state): State<Arc<Container>>,
    Json(payload): Json<InitChatRequest>,
) -> Result<StatusCode, ApiError> {
    payload.validate()?;

    state
        .art_service
        .init_chat(
            &payload.chat_id,
            decode_art(&payload.art)?,
            payload.is_private,
        )
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    info!(
        "Successfully created new chat with id: {}",
        &payload.chat_id
    );

    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    get,
    path = "/v1/messenger/art",
    params(GetARTQuery),
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn get_art(
    State(state): State<Arc<Container>>,
    Query(payload): Query<GetARTQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    if payload.sequence_number.is_none() {
        info!("Check if ART is updating");
        if state
            .art_is_updating
            .read()
            .await
            .contains(&payload.chat_id)
        {
            error!("Failed to retrieve ART, as it is updating");
            return Err(ApiError::from(ARTServiceError::ArtIsChanging));
        }
    }

    let art_record = state
        .art_service
        .get_art(&payload.chat_id, payload.sequence_number)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let art_bytes = art_record
        .art
        .serialize()
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let encoded_art = BASE64_STANDARD.encode(art_bytes);

    Ok((StatusCode::OK, encoded_art))
}

#[utoipa::path(
    post,
    path = "/v1/messenger/add-member",
    request_body = AddMemberRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn add_member(
    State(state): State<Arc<Container>>,
    Json(payload): Json<AddMemberRequest>,
) -> Result<StatusCode, ApiError> {
    payload.validate()?;

    state.start_updating(payload.chat_id).await?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    state
        .art_service
        .append_member(&payload.chat_id, &branch_changes, &payload.proof)
        .await?;

    state.stop_updating(payload.chat_id).await;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    post,
    path = "/v1/messenger/remove-member",
    request_body = RemoveMemberRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn remove_member(
    State(state): State<Arc<Container>>,
    Json(payload): Json<RemoveMemberRequest>,
) -> Result<StatusCode, ApiError> {
    payload.validate()?;

    state.start_updating(payload.chat_id).await?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    state
        .art_service
        .remove_member(&payload.chat_id, &branch_changes, &payload.proof)
        .await?;

    state.stop_updating(payload.chat_id).await;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/v1/messenger/update-key",
    request_body = UpdateKeyRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn update_key(
    State(state): State<Arc<Container>>,
    Json(payload): Json<UpdateKeyRequest>,
) -> Result<StatusCode, ApiError> {
    payload.validate()?;

    state.start_updating(payload.chat_id).await?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    state
        .art_service
        .update_key(&payload.chat_id, &branch_changes, &payload.proof)
        .await?;

    state.stop_updating(payload.chat_id).await;

    Ok(StatusCode::OK)
}

#[utoipa::path(
    get,
    path = "/v1/messenger/changes",
    params(GetChangesQuery),
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn get_changes(
    State(state): State<Arc<Container>>,
    Query(payload): Query<GetChangesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    let filter = doc! {};
    let changes = state
        .art_service
        .list_changes(&payload.chat_id, filter, payload.limit, payload.skip)
        .await?;

    info!("Found changes: {}", changes.len());

    Ok((
        StatusCode::OK,
        postcard::to_allocvec(&changes)
            .map_err(|e| ApiError::InternalServerError(e.to_string()))?,
    ))
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/chat",
    params(DeleteChatQuery),
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn delete_chat(
    State(state): State<Arc<Container>>,
    Query(payload): Query<DeleteChatQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    info!("Delete chat: {}", payload.chat_id);
    state
        .art_service
        .delete_chat(&payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    state.art_is_updating.write().await.remove(&payload.chat_id);
    info!("Deletion is succssesfull");

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get, path = "/v1/messenger/challenge", tag = "Chat operations")]
#[instrument(skip(state), err)]
pub async fn get_challenge(
    State(state): State<Arc<Container>>,
) -> Result<impl IntoResponse, ApiError> {
    info!("Create a write lock on challenges");
    let mut lock = state.challenges.write().await;
    info!("Create new challenge");
    let challenge = (0..10).map(|_| rand::random::<u8>()).collect::<Vec<u8>>();
    lock.insert(challenge.clone());

    Ok((StatusCode::OK, BASE64_STANDARD.encode(challenge)))
}

#[utoipa::path(
    put,
    path = "/v1/messenger/metadata",
    request_body = UpdateMetadataRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state), err)]
pub async fn update_metadata(
    State(state): State<Arc<Container>>,
    Json(payload): Json<UpdateMetadataRequest>,
) -> Result<impl IntoResponse, ApiError> {
    info!("Update metadata: {:?}", payload);
    payload.validate()?;

    state
        .art_service
        .update_metadata(payload.chat_id, payload.metadata, payload.index)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
