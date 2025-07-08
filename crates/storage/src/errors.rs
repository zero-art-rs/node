use art::ARTError;

#[derive(thiserror::Error, Debug)]
pub enum StorageError {
    #[error("Error in mongodb: {0}")]
    MongoDB(#[from] mongodb::error::Error),
    #[error("Failed to retrieve database")]
    DatabaseRetrieval,
    #[error("Failed to retrieve client")]
    ClientRetrieval,
    #[error("Failed to update art")]
    ARTError(#[from] ARTError),
    #[error("Record Not Found")]
    NotFound,
}
