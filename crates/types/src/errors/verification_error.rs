use crate::errors::{ARTServiceError, StorageError};
use art::errors::ARTError;
use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::response::IntoResponse;
use eyre::Report;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("Invalid epoch provided ({provided}), while the current one is {current}")]
    InvalidEpoch { current: u64, provided: u64 },
    #[error("Invalid input Provided")]
    InvalidInput,
    #[error("Failed to use ART {0}")]
    ArtError(#[from] ARTError),
    #[error("ARTServiceError error: {0}")]
    ArtServiceError(#[from] ARTServiceError),
    #[error("Missing query string")]
    MissingQuery,
    #[error("Unknown endpoint")]
    UnknownEndpoint,
    #[error("Invalid message from proof verifier")]
    InvalidResultMessage,
    #[error("Invalid proof")]
    InvalidProof,
    #[error("Failed to send message to proof verifier: {0}")]
    FailedToSendProof(Report),
    #[error("Challenge not found: it may have already been removed")]
    WrongChallenge,
    #[error("ART operation isn't supported")]
    UnsupportedOperation,
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Failed to send message to proof verifier: {0}")]
    AxumError(#[from] axum::Error),
    #[error("Failed to retrieve data from path: {0}")]
    PathRejection(#[from] PathRejection),
    #[error("Failed to retrieve json body: {0}")]
    JsonRejection(#[from] JsonRejection),
    #[error("Failed to decode request: {0}")]
    DecodeError(#[from] prost::DecodeError),
    #[error("Failed to retrieve data from the storage: {0}")]
    StorageError(#[from] StorageError),
}

impl From<serde_json::Error> for VerificationError {
    fn from(error: serde_json::Error) -> Self {
        Self::SerializationError(error.to_string())
    }
}

impl From<ark_serialize::SerializationError> for VerificationError {
    fn from(error: ark_serialize::SerializationError) -> Self {
        Self::SerializationError(error.to_string())
    }
}

impl From<serde_urlencoded::de::Error> for VerificationError {
    fn from(error: serde_urlencoded::de::Error) -> Self {
        Self::SerializationError(error.to_string())
    }
}

impl From<Report> for VerificationError {
    fn from(report: Report) -> Self {
        error!("Failed to send message to proof verifier: {}", report);
        VerificationError::FailedToSendProof(report)
    }
}
