use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use callbacks::callback;
use mongodb::bson::{Binary, spec::BinarySubtype};
use mongodb::bson::{DateTime, doc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, instrument};
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};

use crate::{as_base64, container::Container, errors::ApiError};
use proof_verifier::ProofVerifierSender;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageRequest {
    /// Message content
    #[serde(with = "as_base64")]
    pub message: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde(with = "as_base64")]
    pub nonce: Vec<u8>,

    /// Sequence number of the requested art. If not set, return the latest.
    pub sequence_number: Option<i64>,
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
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn send_message(
    State(state): State<Arc<Container>>,
    Json(payload): Json<SendMessageRequest>,
) -> Result<StatusCode, ApiError> {
    // Validate the request payload.
    info!("Validate payload");
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .messenger_service
        .send_message(payload.message, &payload.chat_id)
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

    // Unique sequence number of the message
    pub message_sequence_number: Option<i64>,

    /// Number of results to be returned
    pub limit: i64,

    /// The amount or results to skip
    pub skip: i64,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde(with = "as_base64")]
    pub nonce: Vec<u8>,

    /// Sequence number of the requested art. If not set, return the latest.
    pub sequence_number: Option<i64>,
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
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn list_messages(
    State(state): State<Arc<Container>>,
    Query(payload): Query<GetMessageQuery>,
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

    if let Some(sequence_number) = payload.message_sequence_number {
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

    // Unique sequence number of the message
    pub sequence_number: Option<i64>,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[serde(with = "as_base64")]
    pub nonce: Vec<u8>,
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
    tag = "Messages"
)]
#[instrument(skip(state), err)]
pub async fn delete_messages(
    State(state): State<Arc<Container>>,
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

    if let Some(sequence_number) = payload.sequence_number {
        _ = filter.insert("sequence_number", sequence_number);
    }

    let removed_messages = state
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
