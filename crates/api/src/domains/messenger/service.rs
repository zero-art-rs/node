use crate::Container;
use chrono::Utc;
use mongodb::bson::Uuid;
use mongodb::bson::{DateTime, Document, doc, from_document, oid::ObjectId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use storage::{MessageStorage, MongoMessageStorage};
use tracing::{debug, error, info};
use types::{CursorRecord, Message};

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

pub struct MessengerService {}

impl MessengerService {
    pub fn new() -> Self {
        Self {}
    }
}

impl MessengerService {
    pub async fn send_message(
        &self,
        message: String,
        chat_id: &Uuid,
    ) -> Result<(), MessengerError> {
        self.get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .store_message(message)
            .await?;
        Ok(())
    }

    pub async fn get_message(
        &self,
        created_at: &DateTime,
        chat_id: &Uuid,
    ) -> Result<Option<Message>, MessengerError> {
        let message = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .get_message(&created_at)
            .await?;
        Ok(message)
    }

    pub async fn get_message_by_id(
        &self,
        id: &str,
        chat_id: &Uuid,
    ) -> Result<Option<Message>, MessengerError> {
        let message_id = match ObjectId::parse_str(id) {
            Ok(id) => id,
            Err(_) => {
                info!("Can't convert given id to ObjectId");
                return Ok(None);
            }
        };

        let message = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .get_message_by_id(&message_id)
            .await?;
        Ok(message)
    }

    pub async fn list_messages(
        &self,
        chat_id: &Uuid,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Message>, MessengerError> {
        let message_record = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .list_messages(limit, skip)
            .await?;
        Ok(message_record)
    }

    pub async fn list_cursors(
        &self,
        chat_id: &Uuid,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<CursorRecord>, MessengerError> {
        let record = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .list_cursors(limit, skip)
            .await?;
        Ok(record)
    }

    pub async fn delete_message(
        &self,
        created_at: &DateTime,
        chat_id: &Uuid,
    ) -> Result<Option<Message>, MessengerError> {
        let result = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .delete_message(&created_at)
            .await?;
        Ok(result)
    }

    pub async fn delete_message_by_id(
        &self,
        id: &str,
        chat_id: &Uuid,
    ) -> Result<Option<Message>, MessengerError> {
        let message_id = match ObjectId::parse_str(id) {
            Ok(id) => id,
            Err(_) => {
                info!("Can't convert given id to ObjectId");
                return Ok(None);
            }
        };

        let result = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .delete_message_by_id(&message_id)
            .await?;
        Ok(result)
    }

    pub async fn mark_as_read(
        &self,
        user_id: &String,
        sequence_number: i64,
        chat_id: &Uuid,
    ) -> Result<Option<CursorRecord>, MessengerError> {
        let result = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .update_user_cursor(user_id, sequence_number)
            .await?;

        Ok(result)
    }

    pub async fn get_storage(
        &self,
        chat_id: &Uuid,
    ) -> Result<Arc<MongoMessageStorage>, mongodb::error::Error> {
        let message_storage = MongoMessageStorage::new(chat_id).await?;
        Ok(Arc::new(message_storage))
    }
}
