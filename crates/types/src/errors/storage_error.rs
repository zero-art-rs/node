use zrt_art::errors::ArtError;

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error("Error in mongodb: {0}")]
    MongoDB(#[from] mongodb::error::Error),
    #[error("Failed to retrieve database")]
    DatabaseRetrieval,
    #[error("Failed to update art")]
    ArtError(#[from] ArtError),
    #[error("Record Not Found")]
    NotFound,
    #[error("Failed to decode payload: {0}")]
    DecodeError(#[from] prost::DecodeError),
    #[error("Failed to encode payload: {0}")]
    EncodeError(#[from] prost::EncodeError),
}
