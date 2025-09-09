use crate::errors::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum MessageServiceError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Group isn't exists yet")]
    GroupNotExists,

    #[error("Failed to decode payload: {0}")]
    DecodeError(#[from] prost::DecodeError),

    #[error("Failed to encode payload: {0}")]
    EncodeError(#[from] prost::EncodeError),
}
