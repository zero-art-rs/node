use crate::errors::{ARTServiceError, MessageServiceError, StorageError, VerificationError};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Invalid input provided")]
    InvalidInput,

    #[error("ARTServiceError: {0}")]
    ARTServiceError(#[from] ARTServiceError),

    #[error("MessageServiceError: {0}")]
    MessageServiceError(#[from] MessageServiceError),

    #[error("Failed to decode payload: {0}")]
    DecodeError(#[from] prost::DecodeError),

    #[error("Aggregation is not implemented.")]
    NotImplemented,

    #[error("MongodbError: {0}.")]
    Mongo(#[from] mongodb::error::Error),

    #[error("StorageError: {0}.")]
    Storage(#[from] StorageError),

    #[error("VerificationError: {0}.")]
    Verification(#[from] VerificationError),
}
