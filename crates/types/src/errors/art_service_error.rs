use crate::errors::StorageError;
use art::errors::ARTError;

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
    #[error("Art is changing, so the result is unpredictable")]
    ArtIsChanging,
    #[error("Failed to initiate new session")]
    SessionInitiation,
    #[error("Failed to use ART {0}")]
    ArtError(#[from] ARTError),
    #[error("No previous record found")]
    NoPreviousRecord,
}
