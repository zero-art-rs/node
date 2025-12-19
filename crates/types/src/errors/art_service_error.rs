use crate::errors::StorageError;
use zrt_art::errors::ArtError;

#[derive(Debug, thiserror::Error)]
pub enum ARTServiceError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("MongoDB error: {0}")]
    MongoDB(#[from] mongodb::error::Error),
    #[error("Unexpected internal error")]
    Internal,
    #[error("Invalid input provided")]
    InvalidInput,
    #[error("Invalid change type provided")]
    InvalidChangeType,
    #[error("Record already exists")]
    AlreadyExists,
    #[error("Record not found")]
    NotFound,
    #[error("The operation can be done only for group chat")]
    GroupChatOnly,
    #[error("Failed to retrieve database")]
    DatabaseRetrieval,
    #[error("Failed to initiate new session")]
    SessionInitiation,
    #[error("ArtError: {0}")]
    ArtError(#[from] ArtError),
    #[error("No previous record found")]
    NoPreviousRecord,
    #[error("Failed to decode payload: {0}")]
    DecodeError(#[from] prost::DecodeError),
    #[error("Fail to perform post verification")]
    FailedPostVerification,
}
