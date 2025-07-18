use mongodb::bson::Document;
use storage::{DataStorage, MessageStorage, MongoMessageStorage, StorageError};
use types::Message;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum MessengerError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub struct MessengerService {}

impl MessengerService {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MessengerService {
    fn default() -> Self {
        Self::new()
    }
}

impl MessengerService {
    pub async fn send_message(
        &self,
        message: Vec<u8>,
        chat_id: &Uuid,
    ) -> Result<(), MessengerError> {
        MongoMessageStorage::new(chat_id)
            .await?
            .store_message(message)
            .await?;
        Ok(())
    }

    pub async fn list_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Message>, MessengerError> {
        let message_record = MongoMessageStorage::new(chat_id)
            .await?
            .list(filter, limit, skip)
            .await?;
        Ok(message_record)
    }

    pub async fn delete_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
    ) -> Result<Vec<Message>, MessengerError> {
        let result = MongoMessageStorage::new(chat_id)
            .await?
            .delete(filter)
            .await?;
        Ok(result)
    }
}
