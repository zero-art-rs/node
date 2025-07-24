use crate::domains::art::service::ARTServiceError;
use art::errors::ARTError;
use eyre::Report;
use tracing::error;

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("Failed to use ART {0}")]
    ArtError(#[from] ARTError),
    #[error("ARTServiceError error: {0}")]
    ArtServiceError(#[from] ARTServiceError),
    #[error("Missing query string")]
    MissingQuery,
    #[error("Unsupported method")]
    UnsupportedMethod,
    #[error("Unknown endpoint")]
    UnknownEndpoint,
    #[error("Invalid message from proof verifier")]
    InvalidResultMessage,
    #[error("Invalid proof")]
    InvalidProof,
    #[error("Failed to send message to proof verifier: {0}")]
    FailedToSendProof(Report),
    #[error("No challenge requested. Use get_challenge endpoint")]
    NoChallenge,
    #[error("ART operation isn't supported")]
    UnsupportedOperation,
    #[error("Serialization error: {0}")]
    SerializationError(String),
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
