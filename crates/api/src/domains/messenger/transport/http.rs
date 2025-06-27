use ark_ec::CurveGroup;
use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use chrono;
use mongodb::bson::{Binary, spec::BinarySubtype};
use mongodb::bson::{DateTime, doc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument};
use zk::curve::cortado::CortadoProjective as ARTG;

use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

use crate::{
    container::Container, domains::auth::transport::http::AuthenticatedUser, errors::ApiError,
};
use art::art::{ART, BranchChanges};

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// Message content
    pub message: String,

    /// Sender public key
    pub sender_public_key: String,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/messages",
    request_body = SendMessageRequest,
    responses(
        (status = 202, description = "Message sent."),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn send_message(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Json(payload): Json<SendMessageRequest>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .messenger_service
        .send_message(payload.message, payload.sender_public_key, &payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::ACCEPTED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetMessageQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Message creation time
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,

    // Content of the message as bytes
    pub content: Option<String>,

    // Unique sequence number of the message
    pub sequence_number: Option<i64>,

    /// Number of results to be returned
    pub limit: i64,

    /// The amount or results to skip
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/messages",
    params(
        GetMessageQuery,
    ),
    responses(
        (status = 202, description = "Message sent."),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn list_messages(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Query(mut payload): Query<GetMessageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut filter = doc! {};

    if let Some(creation_time) = payload.created_at {
        _ = filter.insert(
            "created_at",
            DateTime::from_millis(creation_time.timestamp_millis()),
        );
    }

    if let Some(content) = payload.content {
        _ = filter.insert(
            "content",
            Binary {
                subtype: BinarySubtype::Generic,
                bytes: content.into_bytes(),
            },
        );
    }

    if let Some(sequence_number) = payload.sequence_number {
        _ = filter.insert("sequence_number", sequence_number);
    }

    let messages = state
        .messenger_service
        .list_messages(
            &payload.chat_id,
            filter.clone(),
            payload.limit,
            payload.skip,
        )
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    if messages.is_empty() {
        info!("No messages found for filter {}", filter);
    } else {
        info!("Found next messages for the filter {}", filter);
        for message in &messages {
            info!("Found message: {}", message);
        }
    }

    let response = (StatusCode::OK, Json(messages));

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMessageQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Message creation time
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,

    // Content of the message as bytes
    pub content: Option<String>,

    // Unique sequence number of the message
    pub sequence_number: Option<i64>,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/messages",
    params(
        DeleteMessageQuery,
    ),
    responses(
        (status = 202, description = "Message sent."),
        (status = 204, description = "No Content. Removed successfully."),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn delete_messages(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Query(payload): Query<DeleteMessageQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut filter = doc! {};

    if let Some(creation_time) = payload.created_at {
        _ = filter.insert(
            "created_at",
            DateTime::from_millis(creation_time.timestamp_millis()),
        );
    }

    if let Some(content) = payload.content {
        _ = filter.insert(
            "content",
            Binary {
                subtype: BinarySubtype::Generic,
                bytes: content.into_bytes(),
            },
        );
    }

    if let Some(sequence_number) = payload.sequence_number {
        _ = filter.insert("sequence_number", sequence_number);
    }

    let mut removed_messages = state
        .messenger_service
        .delete_messages(&payload.chat_id, filter.clone())
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let mut status_code = StatusCode::OK;
    if removed_messages.is_empty() {
        info!("No messages found for filter {}", filter);
        status_code = StatusCode::NO_CONTENT;
    } else {
        info!("Found and removed next messages for the filter {}", filter);
        for message in &removed_messages {
            info!("Successfully deleted message: {}", message);
        }
    }

    let response = (status_code, Json(removed_messages));

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct MarkAsRead {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Unique user id
    pub user_id: String,

    /// Message sequence_number to be set for the user
    pub sequence_number: i64,
}

#[utoipa::path(
    put,
    path = "/v1/messenger/cursors",
    params(
        MarkAsRead,
    ),
    responses(
        (status = 202, description = "Message sent."),
        (status = 204, description = "No Content. Remove successfully."),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn mark_as_read(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Query(payload): Query<MarkAsRead>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate the request payload.
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let result = state
        .messenger_service
        .mark_as_read(&payload.user_id, payload.sequence_number, &payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let response;
    match result {
        Some(result) => {
            info!("Successfully read. The previous cursor was: {}", result);
            response = (StatusCode::OK, Json(Some(result)));
        }
        None => response = (StatusCode::NO_CONTENT, Json(None)),
    }

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetCursorsQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// User unique identifier
    pub user_id: Option<String>,

    /// Sequence number of the message, which user already read
    pub cursor: Option<i64>,

    /// Number of results to be returned
    pub limit: i64,

    /// The amount or results to skip
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/cursors",
    params(
        GetCursorsQuery,
    ),
    responses(
        (status = 202, description = "Message sent."),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn list_cursors(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Query(mut payload): Query<GetCursorsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut filter = doc! {};

    if let Some(user_id) = payload.user_id {
        _ = filter.insert("user_id", user_id);
    }

    if let Some(cursor) = payload.cursor {
        _ = filter.insert("cursor", cursor);
    }

    let cursors = state
        .messenger_service
        .list_cursors(
            &payload.chat_id,
            filter.clone(),
            payload.limit,
            payload.skip,
        )
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    if cursors.is_empty() {
        info!("No cursors found for filter {}", filter);
    } else {
        info!("Found next cursors for the filter {}", filter);
        for cursor in &cursors {
            info!("Found cursor: {}", cursor);
        }
    }

    let response = (StatusCode::ACCEPTED, Json(cursors));

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCursorsQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// User unique identifier
    pub user_id: Option<String>,

    /// Sequence number of the message, which user already read
    pub cursor: Option<i64>,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/cursors",
    params(
        DeleteCursorsQuery,
    ),
    responses(
        (status = 202, description = "Message sent."),
        (status = 204, description = "No Content. Removed successfully."),
        (status = 400, description = "Bad request", body = ApiError),
        (status = 401, description = "Unauthorized", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError)
    ),
    security(
        ("bearer_auth" = [])
    ),
    tag = "Messages"
)]
#[instrument(skip(state, headers), err)]
pub async fn delete_cursors(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Query(payload): Query<DeleteCursorsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut filter = doc! {};

    if let Some(user_id) = payload.user_id {
        _ = filter.insert("user_id", user_id);
    }

    if let Some(cursor) = payload.cursor {
        _ = filter.insert("cursor", cursor);
    }

    let mut removed_cursors = state
        .messenger_service
        .delete_cursors(&payload.chat_id, filter.clone())
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let mut status_code = StatusCode::OK;
    if removed_cursors.is_empty() {
        info!("No cursors found for filter {}", filter);
        status_code = StatusCode::NO_CONTENT;
    } else {
        info!("Found and removed cursors for the filter {}:", filter);
        for message in &removed_cursors {
            info!("Successfully deleted cursor: {}", message);
        }
    }

    let response = (status_code, Json(removed_cursors));

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitChatRequest {
    /// Serialised art structure for new chat
    #[schema(
        example = r#"{"root":{"public_key":[93,204,250,239,86,184,225,19,125,201,203,238,144,197,170,153,170,109,111,102,221,244,35,197,34,146,107,80,133,225,225,14,115,37,119,103,215,149,190,117,198,41,73,219,149,186,57,156,21,227,162,136,186,253,134,127,33,135,80,82,37,13,2,143],"l":{"public_key":[163,136,76,181,232,10,167,59,44,1,200,84,84,247,126,197,243,131,169,41,135,0,178,216,144,223,179,188,22,242,36,0,211,222,140,178,157,49,202,100,246,174,122,158,245,208,229,114,94,147,151,183,250,156,150,163,179,110,109,4,33,169,47,137],"l":null,"r":null,"is_temporal":false,"weight":1},"r":{"public_key":[83,85,176,206,106,64,75,186,85,178,13,41,109,63,238,100,32,49,194,89,58,85,147,133,9,198,173,0,22,86,148,8,233,53,14,99,155,164,223,2,19,157,107,220,0,212,197,25,136,253,37,172,73,117,125,45,184,14,253,231,117,82,146,2],"l":null,"r":null,"is_temporal":false,"weight":1},"is_temporal":false,"weight":2},"generator":[22,244,156,151,240,52,186,62,131,201,148,9,140,40,123,188,71,49,218,14,31,201,65,80,246,210,196,99,184,101,78,11,228,198,239,125,93,107,124,64,185,17,37,99,218,201,16,12,183,132,48,197,157,65,225,253,184,205,141,66,95,213,110,139]}"#
    )]
    art: String,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/chat",
    request_body = InitChatRequest,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn init_chat(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Json(payload): Json<InitChatRequest>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let art = ART::<ARTG>::from_json(&payload.art).unwrap();
    let art_pk = art.root.public_key;
    state
        .messenger_service
        .init_chat(&payload.chat_id, art)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    info!(
        "Successfully inited chat. ART root pk: {}",
        art_pk.into_affine()
    );

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetARTQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Sequence number of a tree to retrieve. If not set, retrieve th latest.
    pub sequence_number: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/chat/art",
    params(
        GetARTQuery
    ),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn get_art(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Query(payload): Query<GetARTQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let art = match &payload.sequence_number {
        Some(sequence_number) => {
            info!("Retrieve ART with sequence_number: {}", sequence_number);
            state
                .messenger_service
                .get_art(&payload.chat_id, *sequence_number)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?
                .art
        }
        None => {
            info!("Retrieve the latest ART");
            state
                .messenger_service
                .find_latest_art(&payload.chat_id)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?
                .art
        }
    };

    info!(
        "Retrieved ART with public root key: {}",
        art.root.public_key.into_affine()
    );

    let response = (StatusCode::OK, Json(art));

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetARTChangeQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    // Unique sequence number of the message
    pub sequence_number: Option<i64>,

    /// Number of results to be returned
    pub limit: i64,

    /// The amount or results to skip
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/chat/changes",
    params(
        GetARTChangeQuery
    ),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn list_changes(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Query(payload): Query<GetARTChangeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let mut filter = doc! {};

    if let Some(sequence_number) = &payload.sequence_number {
        _ = filter.insert("sequence_number", sequence_number);
    }

    info!("Retrieve changes with filter: {}", filter);
    let changes = state
        .messenger_service
        .list_changes(&payload.chat_id, filter, payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let response = (StatusCode::OK, Json(changes));

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateARTQuery {
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Serialised structure of BranchChanges
    #[param(
        example = r#"{"change_type":"UpdateKeys","public_keys":[2,0,0,0,0,0,0,0,108,46,5,147,166,28,186,84,56,51,15,0,164,233,163,124,211,62,84,115,12,41,29,21,174,23,41,150,199,190,223,9,32,203,55,0,14,229,209,251,97,64,74,39,27,154,11,74,185,186,109,192,82,15,159,240,224,90,199,170,52,210,1,142,251,48,27,214,6,87,114,21,34,89,30,213,183,242,32,230,13,216,121,58,34,36,192,176,220,239,123,166,185,166,146,2,225,247,234,205,175,76,201,98,151,70,205,212,66,93,245,29,94,100,14,36,206,74,65,202,136,62,39,59,150,107,207,0],"next":["Left"]}"#
    )]
    pub changes: String,
}

#[utoipa::path(
    put,
    path = "/v1/messenger/chat/art",
    params(
        UpdateARTQuery
    ),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn update_art(
    State(state): State<Arc<Container>>,
    AuthenticatedUser(_claims): AuthenticatedUser,
    headers: HeaderMap,
    Query(payload): Query<UpdateARTQuery>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let changes = serde_json::from_str::<BranchChanges<ARTG>>(&payload.changes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .messenger_service
        .update_art(&payload.chat_id, changes.clone())
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    info!(
        "Successfully updated art with changes: {}",
        serde_json::to_string(&changes).unwrap()
    );

    Ok(StatusCode::OK)
}
