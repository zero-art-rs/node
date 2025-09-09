use mongodb::bson::Document;
use storage::{DataStorage, MessageStorage, MongoMessageStorage};
use tracing::debug;
use types::MessageRecord;
use types::errors::MessageServiceError;
use uuid::Uuid;

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
        epoch: i64,
    ) -> Result<(), MessageServiceError> {
        debug!("Store and send new message");
        MongoMessageStorage::new(chat_id)
            .await?
            .store_message(message, epoch)
            .await?;
        debug!("Message sent");
        Ok(())
    }

    pub async fn list_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<MessageRecord>, MessageServiceError> {
        let message_record = MongoMessageStorage::new(chat_id)
            .await?
            .list(filter, None, limit, skip)
            .await?;
        Ok(message_record)
    }

    pub async fn count_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<u64, MessageServiceError> {
        Ok(MongoMessageStorage::new(chat_id)
            .await?
            .count(filter, limit, skip)
            .await?)
    }

    pub async fn delete_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
    ) -> Result<Vec<MessageRecord>, MessageServiceError> {
        let result = MongoMessageStorage::new(chat_id)
            .await?
            .delete(filter)
            .await?;
        Ok(result)
    }
}
