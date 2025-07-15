use art::errors::ARTError;
use axum::{Json, http::StatusCode, response::IntoResponse};
use core::fmt;

use crate::domains::art::service::ARTServiceError;
use serde_json::json;
use tracing::error;
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
            | ARTServiceError::NotFound
            | ARTServiceError::GroupChatOnly => Self::BadRequest(value.to_string()),
            _ => Self::InternalServerError(value.to_string()),
        }
    }
}

impl From<validator::ValidationErrors> for ApiError {
    fn from(value: validator::ValidationErrors) -> Self {
        Self::BadRequest(value.to_string())
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
