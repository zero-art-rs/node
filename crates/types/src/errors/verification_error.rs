use crate::errors::{ARTServiceError, ApiError, MessageServiceError, ServiceError, StorageError};
use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::response::IntoResponse;
use eyre::Report;
use tracing::{debug, error};
use zrt_art::errors::ArtError;

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("Invalid epoch provided ({provided}), while the current one is {current}")]
    InvalidEpoch { current: u64, provided: u64 },
    #[error("Invalid input Provided")]
    InvalidInput,
    #[error("ArtError: {0}")]
    ArtError(#[from] ArtError),
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
    #[error("AddMember operation must be unique for epoch, but epoch {epoch} already has one.")]
    AddMemberUniqueness { epoch: u64 },
    #[error("Exclusive operation for epoch: {0} already exists.")]
    ExclusiveOperationAlreadyExists(u64),
    #[error("Can't perform operation as the user is already removed.")]
    UserAlreadyRemoved,
    #[error("Aggregation isn't supported yet.")]
    UnsupportedAggregation,
    #[error("Cant merge change to increase epoch {0}, as there are changes for this epoch.")]
    UnsupportedMerge(u64),
    #[error("Can't remove the same user several times at the same epoch or cant update his key.")]
    MergeUserRemove,
    #[error("Can't remove the same user several times at the same epoch.")]
    Postcard(#[from] postcard::Error),
    #[error("MessageServiceError: {0}")]
    MessageService(#[from] MessageServiceError),
    #[error("Fail to update ART. I is changing now.")]
    ArtIsUpdating,
    #[error("MongoDB error: {0}.")]
    Mongo(#[from] mongodb::error::Error),
    #[error("NotFound")]
    NotFound,
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

impl IntoResponse for VerificationError {
    fn into_response(self) -> axum::response::Response {
        debug!("Verification response: Error: {:?}", self);
        ApiError::from(self).into_response()
    }
}
