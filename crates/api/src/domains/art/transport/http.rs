use crate::domains::art::service::ARTServiceError;
use crate::domains::art::transport::utils::{as_base64, decode_art, decode_branch_changes};
use crate::{container::Container, errors::ApiError};
use art::{traits::ARTPublicAPI, types::BranchChangesType};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitChatRequest {
    /// Serialised art structure for new chat.
    #[schema(
        example = "QHVOIsS7aF9klJHKUrxekAPKV+33NbmB4J5NK/mh6IQEGL8+nmZHd7rRwbWDOGRq0g9woVvX+rkmxu0tUHNIPYwBQMw5OnIemJkFNHnm8HCsN+99ekIxBgYotCVAYwdDPZAP5HTFQe45VuD23SI2mxW8D8j3KknDPepDEmg8n0j+1AIBQOuX8YX/e0i16YbSJ2lpURvM+0QcuToiM9UyBPvadnEDbZNGFdiQL3ULmmOtRtL2+BP9DTmBeNxx3fhYGU95FAIBQH/P/s7p2KJZctpWuukfHNEAK/oQrZfQ0j5hs+Qm6ecEaiBYdyjJ1FggyqqDkbfDEsjtebcPuZhp5u/cEQtd2wEAAAABAAFAahDbRHi2H9n/6hkYlFvT+EykOrS3Hd9fe9/yhehPeAx458pBSNKYPFXt+zjMTnrm4EMBYPy1boslBDoZBC+cAAAAAAEAAAIAAUCRRwLnU975oL83p+ZFh/zi6+8IfyqHlX9mZ3f6rVU6As4ao2I5tVFmfuuqhkQBjIZRxMfm/rSOIA4yrF5/53mMAUC9TfgZxLFEsOVYQsrnA59gb2Vr6qYW2PaMXfM/aTclAdcCotgt00eyxNt+EgWJ8JrjVVxZIt6RMzmvnmz0NwSPAAAAAQABQPVlwjO+3CsDj5QABAm2nwGO5ZEkZ8SCnzQK1m0tG/INK8L/ZBea3aK0weGxV2aWDtZul1Zf72mO+udih9HdP44AAAABAAACAAAEAAFA0DtiBydl146UurHWfv/bO6YSBkVWLxG3Cpxykz7ZJwJu6sGgmhc5gBRTs5Wq6+fZZh4+iplHCO+PY71NIrXOBwFASx7wVwl1/xvn4u0LYvN0XqsJxtgKehRInq7TO1UEyQ6UHnYb/1oa1sIlHrOs2GoMS5RdFCr8TybWxZDobwTRAwFAyiSMcuKbWNbI1809Y7g3neYpF4+BnhLWmU9Ea3oWOgoIeNPNQKQJ9TQ9eXOzmmCfXw81UGtrE6z39p/fyG+LAwAAAAEAAUBr5E3m12kSS8uDciloIUALp+VvI77c+54dtMdwaT/jDruUOc20HJ2CIbUs3b7cDA8Gae1bHVcGDDXOxB8HHaOPAAAAAQAAAgABQE2+Ilu7OJj6fMBlJLUmYmHU/rQO0U4nRwKyRH7CH6wNzxvjyz3rYSXq1rm72pt8hwJ4vKZtIEv23rVnuZ86p4sBQOZJ/qh4RDUjWBl9010Pa+YP2Kq781NS23Gk6/h6BxYNmT2emqCBuimYo+xrTLDFslQdpWuOQSYaZz+d0u9264wAAAABAAFAjE/8rW2OG9VVZ4vmzKuE0bFnjgMzSnNtkxgh1752lwxFI56bmXf95h4ZE9RRrup+AAE/qU7lpBJBsuqJwllAjgAAAAEAAAIAAAQAAAgAQBb0nJfwNLo+g8mUCYwoe7xHMdoOH8lBUPbSxGO4ZU4L5MbvfV1rfEC5ESVj2skQDLeEMMWdQeH9uM2NQl/Vbos="
    )]
    #[serde(with = "as_base64")]
    art: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,

    /// Indicates whether the chat is private (one to one).
    #[schema(example = false)]
    pub is_private: bool,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/init-chat",
    request_body = InitChatRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn init_chat(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Json(payload): Json<InitChatRequest>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let art = decode_art(&payload.art)?;

    state
        .art_service
        .init_chat(&payload.chat_id, art, payload.is_private)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::CREATED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetARTQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,

    /// Sequence number of the requested art. If not set, return the latest.
    #[param(example = 1)]
    pub sequence_number: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/art",
    params(GetARTQuery,),
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn get_art(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Query(payload): Query<GetARTQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let art = match &payload.sequence_number {
        Some(sequence_number) => {
            let mut initial_art = state
                .art_service
                .get_initial_art(&payload.chat_id)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?
                .art;

            let filter = doc! { "sequence_number": { "$lt": sequence_number } };
            let mut changes = state
                .art_service
                .list_changes(&payload.chat_id, filter, *sequence_number, 0)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

            if changes.len() < *sequence_number as usize {
                return Err(ApiError::InternalServerError(
                    ARTServiceError::NotFound.to_string(),
                ));
            }

            changes.sort_by(|a, b| a.sequence_number.cmp(&b.sequence_number));

            for change in &changes {
                initial_art
                    .update_public_art(&change.changes)
                    .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
            }

            initial_art
        }
        None => {
            state
                .art_service
                .get_art(&payload.chat_id)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?
                .art
        }
    };

    let art_bytes = art
        .serialize()
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let encoded_art = BASE64_STANDARD.encode(art_bytes);

    Ok((StatusCode::OK, encoded_art))
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberRequest {
    /// Serialised BranchChanges:AppendNode structure.
    #[schema(
        example = "AUB0urTTqwXgQt9FnyA0DzPCkHbfZPx5Tnbtu4ApwNNAAt3A68AFXBv6RU+dOJ2uft6tRml2W6HSPktlP3PS66iNAAAAAQDIAgUAAAAAAAAA2vrTKjDNUuMjI/9/VQBjWMsycwz55AGdDgml5yXHKwkxwmm+Fb+26mRv7d/Wfhi9wpscXZHFXBQZXwgqod8Kioqy60ZImTb3xVNlt74eTzEOmAXKK5uQ2cHzPr6cITcHKmzAqUY8f5s2mRWBDHOsl1/BrUVmH4aKzhBZCGyoBgIRdfOd/IRv0iwex8PFx/V+DaFVTLxhnRcGWk0qaA4bDlwJy0+22eqpdO8xxgc4hsBgz9ImAlz9h7ZjhqbJ10eOJzYF1QxsgHS6g7wsHjJPZqxxkq4mN8vLatzsfnP58QNHfTNlqUMIhXD5IcgR+UELutN9AIxqt0sKTGJCAj8aAHS6tNOrBeBC30WfIDQPM8KQdt9k/HlOdu27gCnA00AC3cDrwAVcG/pFT504na5+3q1GaXZbodI+S2U/c9LrqI0ACA=="
    )]
    #[serde(with = "as_base64")]
    branch_changes: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/add-member",
    request_body = AddMemberRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn add_member(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Json(payload): Json<AddMemberRequest>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    match branch_changes.change_type {
        BranchChangesType::AppendNode(_) => {
            state
                .art_service
                .update_art(payload.chat_id, &branch_changes)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
        }
        _ => {
            return Err(ApiError::BadRequest(
                "Invalid change type. Expected new member addition".to_owned(),
            ));
        }
    }

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMember {
    /// Serialised BranchChanges:UpdateKeys structure.
    #[schema(
        example = r#"AEBqENtEeLYf2f/qGRiUW9P4TKQ6tLcd31973/KF6E94DHjnykFI0pg8Ve37OMxOeubgQwFg/LVuiyUEOhkEL5wAIDef4VDfd4IpU5zytQEbb07HasgN+7uh8Nuy+Z9sKqAHiAIEAAAAAAAAAOe9K4Vv22xfOUmaL9Lw+/zDisVnTsJcitnAXAiCz38ESx1h3sa/HXUURlpHmKnzwkeSGL0gbTnUNzbjhkSIIohNxl5CFfLAQ2DfRt+Z+egwjgAGY9H22ly4R8I0ekkHC33f34mqKtdccZfRbfnTvzqE/inTg2IAO2BqPQz+lKGP3lpeFTKKD5neWN8xOjEJgJhVZuntoYoWXFmCuZeB6wCXbogsc8vcPKNGte/jiAb7VWfyvgF/vja9v7eusCcziuj2vYhZaT9ROhTy+jhbZDeKcdT2dzo1SvqL3VhbLyMHJkRBbh1WKWhshFlWRLrDN8OohOIxfyIfKTnuNJKhgYwACQ=="#
    )]
    #[serde(with = "as_base64")]
    branch_changes: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/remove-member",
    request_body = RemoveMember,
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn remove_member(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Json(payload): Json<RemoveMember>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    match branch_changes.change_type {
        BranchChangesType::MakeBlank(_, _) => {
            state
                .art_service
                .update_art(payload.chat_id, &branch_changes)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
        }
        _ => {
            return Err(ApiError::BadRequest(
                "Invalid change type. Expected RemoveNode".to_owned(),
            ));
        }
    }

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateKey {
    /// Serialised BranchChanges:UpdateKeys structure
    #[schema(
        example = r#"AogCBAAAAAAAAAD55GV+qsCIdq7XQocrftP67C2v+IzRh/2bCusqV/vTBT5mt6GohPQVTqKwwq8XbmK+q4SK5t+lblT6+aLF52+J9UoPL4SNMEtSwwBTv0ogZ7RDvzc1qlgapuQuwcBZrw1R9f9B3pf/4T2gp1eWz09JTmw2eoSGwMCsmlofQj9/BXMQjS0HYKiqp7A54v7YXC+ptl7n5A1xLmF3vb8tFDQNPDR0TIypJKk0y5UoKK8OMt9MDapD3Q9DCnfewAOhb4tkJ4WKL6MWoGmIjuDwV0+LXrw5T/5thbW+/pDQb+35DaWE+LtAKNjKamPHU50SJYTKe8QLu+kXQLElBFPM9dIBAAo="#
    )]
    #[serde(with = "as_base64")]
    branch_changes: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/update-key",
    request_body = UpdateKey,
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn update_key(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Json(payload): Json<UpdateKey>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    match branch_changes.change_type {
        BranchChangesType::UpdateKey => {
            state
                .art_service
                .update_art(payload.chat_id, &branch_changes)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
        }
        _ => {
            return Err(ApiError::BadRequest(
                "Invalid change type. Expected UpdateKeys.".to_owned(),
            ));
        }
    }

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetChangesQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,

    /// Number of results to be returned.
    #[param(example = 10)]
    pub limit: i64,

    /// The amount or results to skip at first.
    #[param(example = 0)]
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/changes",
    params(GetChangesQuery),
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn get_changes(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Query(payload): Query<GetChangesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let filter = doc! {};
    let changes = state
        .art_service
        .list_changes(&payload.chat_id, filter, payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    info!("Found changes: {}", changes.len());

    Ok((
        StatusCode::OK,
        postcard::to_allocvec(&changes)
            .map_err(|e| ApiError::InternalServerError(e.to_string()))?,
    ))
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteChatQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/chat",
    params(DeleteChatQuery,),
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn delete_chat(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Query(payload): Query<DeleteChatQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .art_service
        .delete_chat(&payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::OK)
}
