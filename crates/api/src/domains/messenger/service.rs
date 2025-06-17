use chrono::Utc;
use mongodb::bson::Uuid;
use mongodb::bson::{DateTime, Document, doc, from_document, oid::ObjectId};
use serde::{Deserialize, Serialize};
use storage::MessageStorage;
use tracing::{debug, error, info};
use types::Message;

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
        self.storage
            .store_message(Message::new(message.into_bytes()))
            .await?;
        Ok(())
    }

    pub async fn get_message(
        &self,
        created_at: &DateTime,
    ) -> Result<Option<Message>, MessengerError> {
        let message = self.storage.get_message(&created_at).await?;
        Ok(message)
    }

    pub async fn get_message_by_id(&self, id: &str) -> Result<Option<Message>, MessengerError> {
        let message_id = match ObjectId::parse_str(id) {
            Ok(id) => id,
            Err(_) => {
                info!("Can't convert given id to ObjectId");
                return Ok(None);
            }
        };

        let message = self.storage.get_message_by_id(&message_id).await?;
        Ok(message)
    }

    pub async fn list_messages(&self) -> Result<Vec<Message>, MessengerError> {
        let message_record = self.storage.list_messages(10, 0).await?;
        Ok(message_record)
    }

    pub async fn delete_message(
        &self,
        created_at: &DateTime,
    ) -> Result<Option<Message>, MessengerError> {
        let result = self.storage.delete_message(&created_at).await?;
        Ok(result)
    }

    pub async fn delete_message_by_id(&self, id: &str) -> Result<Option<Message>, MessengerError> {
        let message_id = match ObjectId::parse_str(id) {
            Ok(id) => id,
            Err(_) => {
                info!("Can't convert given id to ObjectId");
                return Ok(None);
            }
        };

        let result = self.storage.delete_message_by_id(&message_id).await?;
        Ok(result)
    }
}
