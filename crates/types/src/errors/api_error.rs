use axum::{Json, http::StatusCode, response::IntoResponse};
use core::fmt;
use zrt_art::errors::ARTError;

use crate::errors::{ARTServiceError, MessageServiceError, ServiceError};
use serde_json::json;
use utoipa::ToSchema;

/// Errors that can occur in the API
#[derive(Debug, ToSchema)]
pub enum ApiError {
    /// Bad request
    BadRequest(String),
    /// Unauthorized
    Unauthorized(String),
    /// Internal server error
    InternalServerError(String),
    /// Resource not found
    NotFound(String),
}

impl From<ServiceError> for ApiError {
    fn from(value: ServiceError) -> Self {
        match value {
            ServiceError::ARTServiceError(art_err) => Self::from(art_err),
            ServiceError::MessageServiceError(msg_err) => Self::from(msg_err),
            ServiceError::DecodeError(_) => ApiError::BadRequest(String::from("Invalid request")),
            ServiceError::ArtIsUpdating => ApiError::InternalServerError(value.to_string()),
            ServiceError::NotImplemented => ApiError::BadRequest(String::from("Invalid request")),
        }
    }
}

impl From<ark_serialize::SerializationError> for ApiError {
    fn from(err: ark_serialize::SerializationError) -> Self {
        ApiError::BadRequest(err.to_string())
    }
}

impl From<ARTError> for ApiError {
    fn from(value: ARTError) -> Self {
        Self::InternalServerError(value.to_string())
    }
}

impl From<ARTServiceError> for ApiError {
    fn from(value: ARTServiceError) -> Self {
        match value {
            ARTServiceError::AlreadyExists
            | ARTServiceError::InvalidInput
            | ARTServiceError::InvalidChangeType
            | ARTServiceError::GroupChatOnly => Self::BadRequest(value.to_string()),
            ARTServiceError::NotFound => Self::NotFound(value.to_string()),
            _ => Self::InternalServerError(value.to_string()),
        }
    }
}

impl From<MessageServiceError> for ApiError {
    fn from(value: MessageServiceError) -> Self {
        match value {
            MessageServiceError::GroupNotExists => ApiError::BadRequest(value.to_string()),
            MessageServiceError::Storage(err) => ApiError::InternalServerError(err.to_string()),
            MessageServiceError::DecodeError(err) => ApiError::InternalServerError(err.to_string()),
            MessageServiceError::EncodeError(err) => ApiError::InternalServerError(err.to_string()),
        }
    }
}

impl From<validator::ValidationErrors> for ApiError {
    fn from(value: validator::ValidationErrors) -> Self {
        Self::BadRequest(value.to_string())
    }
}

impl From<crate::errors::VerificationError> for ApiError {
    fn from(value: crate::errors::VerificationError) -> Self {
        Self::Unauthorized(value.to_string())
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadRequest(msg) => write!(f, "Bad Request: {msg}"),
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {msg}"),
            Self::InternalServerError(msg) => write!(f, "Internal Server Error: {msg}"),
            Self::NotFound(msg) => write!(f, "Not Found: {msg}"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_message) = match self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            Self::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
        };
        (status, Json(json!({ "error": error_message }))).into_response()
    }
}
