use crate::{container::Container, errors::ApiError};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::{SeedableRng, rngs::StdRng};
use ark_std::{One, UniformRand, Zero};
use art::{ART, BranchChanges, BranchChangesType};
use axum::extract::{Path, Query};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{self, HeaderMap, Response, StatusCode},
    response::IntoResponse,
};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chrono::NaiveDateTime;
use mongodb::bson::Uuid;
use mongodb::bson::doc;
use postcard;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument};
use tracing_subscriber::fmt::init;
use utoipa::{IntoParams, ToSchema};
use validator::Validate;
use zk::curve::cortado::CortadoAffine as ARTGroup;

/// Decode branch changes from base64 string
pub fn decode_branch_changes(branch_changes: &String) -> Result<BranchChanges<ARTGroup>, ApiError> {
    let branch_changes_bytes = BASE64_STANDARD
        .decode(branch_changes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    BranchChanges::<ARTGroup>::deserialize(&branch_changes_bytes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))
}

/// Decode art from base64 string
pub fn decode_art(art: &str) -> Result<ART<ARTGroup>, ApiError> {
    let art_bytes = BASE64_STANDARD
        .decode(art)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    ART::<ARTGroup>::deserialize_with_postcard(&art_bytes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitChatRequest {
    /// Postman serialised art structure for new chat
    #[schema(
        example = "QHVOIsS7aF9klJHKUrxekAPKV+33NbmB4J5NK/mh6IQEGL8+nmZHd7rRwbWDOGRq0g9woVvX+rkmxu0tUHNIPYwBQMw5OnIemJkFNHnm8HCsN+99ekIxBgYotCVAYwdDPZAP5HTFQe45VuD23SI2mxW8D8j3KknDPepDEmg8n0j+1AIBQOuX8YX/e0i16YbSJ2lpURvM+0QcuToiM9UyBPvadnEDbZNGFdiQL3ULmmOtRtL2+BP9DTmBeNxx3fhYGU95FAIBQH/P/s7p2KJZctpWuukfHNEAK/oQrZfQ0j5hs+Qm6ecEaiBYdyjJ1FggyqqDkbfDEsjtebcPuZhp5u/cEQtd2wEAAAABAUBqENtEeLYf2f/qGRiUW9P4TKQ6tLcd31973/KF6E94DHjnykFI0pg8Ve37OMxOeubgQwFg/LVuiyUEOhkEL5wAAAAAAQACAUCRRwLnU975oL83p+ZFh/zi6+8IfyqHlX9mZ3f6rVU6As4ao2I5tVFmfuuqhkQBjIZRxMfm/rSOIA4yrF5/53mMAUC9TfgZxLFEsOVYQsrnA59gb2Vr6qYW2PaMXfM/aTclAdcCotgt00eyxNt+EgWJ8JrjVVxZIt6RMzmvnmz0NwSPAAAAAQFA9WXCM77cKwOPlAAECbafAY7lkSRnxIKfNArWbS0b8g0rwv9kF5rdorTB4bFXZpYO1m6XVl/vaY7652KH0d0/jgAAAAEAAgAEAUDQO2IHJ2XXjpS6sdZ+/9s7phIGRVYvEbcKnHKTPtknAm7qwaCaFzmAFFOzlarr59lmHj6KmUcI749jvU0itc4HAUBLHvBXCXX/G+fi7Qti83ReqwnG2Ap6FEiertM7VQTJDpQedhv/WhrWwiUes6zYagxLlF0UKvxPJtbFkOhvBNEDAUDKJIxy4ptY1sjXzT1juDed5ikXj4GeEtaZT0RrehY6Cgh4081ApAn1ND15c7OaYJ9fDzVQa2sTrPf2n9/Ib4sDAAAAAQFAa+RN5tdpEkvLg3IpaCFAC6flbyO+3PueHbTHcGk/4w67lDnNtBydgiG1LN2+3AwPBmntWx1XBgw1zsQfBx2jjwAAAAEAAgFATb4iW7s4mPp8wGUktSZiYdT+tA7RTidHArJEfsIfrA3PG+PLPethJerWubvam3yHAni8pm0gS/betWe5nzqniwFA5kn+qHhENSNYGX3TXQ9r5g/YqrvzU1LbcaTr+HoHFg2ZPZ6aoIG6KZij7GtMsMWyVB2la45BJhpnP53S73brjAAAAAEBQIxP/K1tjhvVVWeL5syrhNGxZ44DM0pzbZMYIde+dpcMRSOem5l3/eYeGRPUUa7qfgABP6lO5aQSQbLqicJZQI4AAAABAAIABAAIQBb0nJfwNLo+g8mUCYwoe7xHMdoOH8lBUPbSxGO4ZU4L5MbvfV1rfEC5ESVj2skQDLeEMMWdQeH9uM2NQl/Vbos="
    )]
    art: String,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = true)]
    pub is_private: bool,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/init-chat",
    request_body = InitChatRequest,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn init_chat(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
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
}

#[utoipa::path(
    get,
    path = "/v1/messenger/art",
    params(
        GetARTQuery,
    ),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn get_art(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<GetARTQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let art = state
        .art_service
        .get_art(&payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let art_bytes = art
        .art
        .serialise_with_postcard()
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let encoded_art = BASE64_STANDARD.encode(art_bytes);

    let response = (StatusCode::OK, encoded_art);

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberRequest {
    /// Postman serialised BranchChanges:AppendNode structure
    #[schema(
        example = "AUB0urTTqwXgQt9FnyA0DzPCkHbfZPx5Tnbtu4ApwNNAAt3A68AFXBv6RU+dOJ2uft6tRml2W6HSPktlP3PS66iNAAAAAcgCBQAAAAAAAADa+tMqMM1S4yMj/39VAGNYyzJzDPnkAZ0OCaXnJccrCTHCab4Vv7bqZG/t39Z+GL3CmxxdkcVcFBlfCCqh3wqKirLrRkiZNvfFU2W3vh5PMQ6YBcorm5DZwfM+vpwhNwcqbMCpRjx/mzaZFYEMc6yXX8GtRWYfhorOEFkIbKgGAhF18538hG/SLB7Hw8XH9X4NoVVMvGGdFwZaTSpoDhsOXAnLT7bZ6ql07zHGBziGwGDP0iYCXP2HtmOGpsnXR44nNgXVDGyAdLqDvCweMk9mrHGSriY3y8tq3Ox+c/nxA0d9M2WpQwiFcPkhyBH5QQu6030AjGq3SwpMYkICPxoAdLq006sF4ELfRZ8gNA8zwpB232T8eU527buAKcDTQALdwOvABVwb+kVPnTidrn7erUZpdluh0j5LZT9z0uuojQQBAQEC"
    )]
    branch_changes: String,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/add-member",
    request_body = AddMemberRequest,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn add_member(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
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
                .add_user(&payload.chat_id, &branch_changes)
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
    /// Postman serialised BranchChanges:UpdateKeys structure
    #[schema(
        example = r#"AEBqENtEeLYf2f/qGRiUW9P4TKQ6tLcd31973/KF6E94DHjnykFI0pg8Ve37OMxOeubgQwFg/LVuiyUEOhkEL5wAIDef4VDfd4IpU5zytQEbb07HasgN+7uh8Nuy+Z9sKqAHiAIEAAAAAAAAAOe9K4Vv22xfOUmaL9Lw+/zDisVnTsJcitnAXAiCz38ESx1h3sa/HXUURlpHmKnzwkeSGL0gbTnUNzbjhkSIIohNxl5CFfLAQ2DfRt+Z+egwjgAGY9H22ly4R8I0ekkHC33f34mqKtdccZfRbfnTvzqE/inTg2IAO2BqPQz+lKGP3lpeFTKKD5neWN8xOjEJgJhVZuntoYoWXFmCuZeB6wCXbogsc8vcPKNGte/jiAb7VWfyvgF/vja9v7eusCcziuj2vYhZaT9ROhTy+jhbZDeKcdT2dzo1SvqL3VhbLyMHJkRBbh1WKWhshFlWRLrDN8OohOIxfyIfKTnuNJKhgYwDAQEC"#
    )]
    branch_changes: String,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/remove-member",
    request_body = RemoveMember,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn remove_member(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<RemoveMember>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    match branch_changes.change_type {
        BranchChangesType::MakeTemporal(_, _) => {
            state
                .art_service
                .update_art(&payload.chat_id, &branch_changes)
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
pub struct LeaveChat {
    /// Serialised art structure for new chat
    // #[schema(example = r#""#)]
    leave_message: String,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/leave",
    request_body = LeaveChat,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn leave_chat(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<LeaveChat>,
) -> Result<StatusCode, ApiError> {
    todo!();
    // payload
    //     .validate()
    //     .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    //
    // // state.messenger_service.send_message(payload.leave_message)
    //
    // Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateKey {
    /// Postman serialised BranchChanges:UpdateKeys structure
    #[schema(
        example = r#"AogCBAAAAAAAAAD55GV+qsCIdq7XQocrftP67C2v+IzRh/2bCusqV/vTBT5mt6GohPQVTqKwwq8XbmK+q4SK5t+lblT6+aLF52+J9UoPL4SNMEtSwwBTv0ogZ7RDvzc1qlgapuQuwcBZrw1R9f9B3pf/4T2gp1eWz09JTmw2eoSGwMCsmlofQj9/BXMQjS0HYKiqp7A54v7YXC+ptl7n5A1xLmF3vb8tFDQNPDR0TIypJKk0y5UoKK8OMt9MDapD3Q9DCnfewAOhb4tkJ4WKL6MWoGmIjuDwV0+LXrw5T/5thbW+/pDQb+35DaWE+LtAKNjKamPHU50SJYTKe8QLu+kXQLElBFPM9dIBAwECAQ=="#
    )]
    branch_changes: String,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/update-key",
    request_body = UpdateKey,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn update_key(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<UpdateKey>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    match branch_changes.change_type {
        BranchChangesType::UpdateKeys => {
            state
                .art_service
                .update_art(&payload.chat_id, &branch_changes)
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

    /// Number of results to be returned
    #[param(example = 10)]
    pub limit: i64,

    /// The amount or results to skip at first
    #[param(example = 0)]
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/changes",
    params(GetChangesQuery),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn get_changes(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
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

    let response_body = postcard::to_allocvec(&changes)
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    let response = (StatusCode::OK, response_body);

    Ok(response)
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
    params(
        DeleteChatQuery,
    ),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn delete_chat(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
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

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListChats {
    /// Number of results to be returned
    #[param(example = 10)]
    pub limit: i64,

    /// The amount or results to skip at first
    #[param(example = 0)]
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/chats",
    params(
        ListChats,
    ),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn list_chats(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<ListChats>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let chats_ids = state
        .art_service
        .list_chats(payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    for chat_id in &chats_ids {
        info!("Found chat {:?}", chat_id);
    }

    let body = postcard::to_allocvec(&chats_ids)
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    let response = (StatusCode::OK, body);

    Ok(response)
}
