use mongodb::bson::{DateTime, Document, doc, from_document, oid::ObjectId};
use serde::{Deserialize, Serialize};
use storage::MessageStorage;
use tracing::{debug, error, info};

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub content: String,
    pub created_at: DateTime,
    pub sender_id: String,
}

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
            .store_message(Message {
                content: message,
                created_at: DateTime::now(),
                sender_id: "temps".to_string(),
            })
            .await?;
        Ok(())
    }

    pub async fn get_message(&self, id: String) -> Result<Option<Document>, MessengerError> {
        let message_id = match ObjectId::parse_str(id) {
            Ok(id) => id,
            Err(_) => {
                info!("Can't convert given id to ObjectId");
                return Ok(None)
            },
        };

        let message = self.storage.get_message(&message_id).await?;
        Ok(message)
    }

    pub async fn list_messages(&self) -> Result<Vec<Document>, MessengerError> {
        let message_record = self.storage.list_messages(10, 0).await?;
        Ok(message_record)
    }

    pub async fn delete_messages(&self, id: String) -> Result<Option<Document>, MessengerError> {
        let message_id = match ObjectId::parse_str(id) {
            Ok(id) => id,
            Err(_) => {
                info!("Can't convert given id to ObjectId");
                return Ok(None)
            },
        };

        let result = self.storage.delete_message(&message_id).await?;
        Ok(result)
    }
}
