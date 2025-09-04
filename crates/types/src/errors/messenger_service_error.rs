use crate::errors::StorageError;

#[derive(Debug, thiserror::Error)]
pub enum MessengerError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    
    #[error("Group isn't exists yet")]
    GroupNotExists,
}
