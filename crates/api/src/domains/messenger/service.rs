use storage::MessageStorage;

#[derive(Debug, thiserror::Error)]
pub enum MessengerError {
    #[error("Storage error: {0}")]
    StorageError(storage::Error),
}

impl From<storage::Error> for MessengerError {
    fn from(error: storage::Error) -> Self {
        MessengerError::StorageError(error)
    }
}

pub struct MessengerService<S> {
    storage: S,
}

impl<S> MessengerService<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl<S> MessengerService<S>
where
    S: MessageStorage + Send + Sync + 'static,
{
    pub async fn send_message(&self, message: String) -> Result<(), MessengerError> {
        self.storage.store_message(message).await?;
        Ok(())
    }
}
