use crate::Container;
use chrono::Utc;
use mongodb::bson::Uuid;
use mongodb::bson::{DateTime, Document, doc, from_document, oid::ObjectId};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use mongodb::bson;
use storage::{MessageStorage, MongoMessageStorage};
use tracing::{debug, error, info};
use types::{CursorRecord, Message};

#[derive(Debug, thiserror::Error)]
pub enum MessengerError {
    #[error("Storage error: {0}")]
    StorageError(storage::Error),
    #[error("Conversion error: {0}")]
    ConversionError(bson::oid::Error)
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
        sender: String,
        chat_id: &Uuid,
    ) -> Result<(), MessengerError> {
        self.get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .store_message(message, sender)
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
        let message_record = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .list_messages(filter, limit, skip)
            .await?;
        Ok(message_record)
    }

    pub async fn list_cursors(
        &self,
        chat_id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<CursorRecord>, MessengerError> {
        let record = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .list_cursors(filter, limit, skip)
            .await?;
        Ok(record)
    }

    pub async fn delete_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
    ) -> Result<Vec<Message>, MessengerError> {
        let result = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .delete_messages(filter)
            .await?;
        Ok(result)
    }

    pub async fn delete_cursors(
        &self,
        chat_id: &Uuid,
        filter: Document,
    ) -> Result<Vec<CursorRecord>, MessengerError> {
        let result = self
            .get_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .delete_cursors(filter)
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
