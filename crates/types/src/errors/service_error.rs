use crate::errors::{ARTServiceError, MessageServiceError};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("ARTServiceError: {0}")]
    ARTServiceError(#[from] ARTServiceError),

    #[error("MessageServiceError: {0}")]
    MessageServiceError(#[from] MessageServiceError),

    #[error("Failed to decode payload: {0}")]
    DecodeError(#[from] prost::DecodeError),

    #[error("Fail to update ART. I is changing now.")]
    ArtIsUpdating,
}
