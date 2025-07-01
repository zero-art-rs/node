use art::{ART, BranchChanges};
use mongodb::bson;
use mongodb::bson::{DateTime, Document, doc, from_document, oid::ObjectId};
use std::sync::Arc;
use storage::{
    CursorStorage, DataStorage, MessageStorage, MongoCursorStorage, MongoMessageStorage,
};
use tracing::{debug, error, info};
use types::{ARTChangesRecord, ARTRecord, CursorRecord, Message};
use uuid::Uuid;
use zk::curve::cortado::CortadoProjective as ARTG;

#[derive(Debug, thiserror::Error)]
pub enum MessengerError {
    #[error("Storage error: {0}")]
    StorageError(storage::Error),
    #[error("Conversion error: {0}")]
    ConversionError(bson::oid::Error),
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
        self.create_messages_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .store_message(message.into_bytes(), sender)
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
            .create_messages_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .list(filter, limit, skip)
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
            .create_cursors_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .list(filter, limit, skip)
            .await?;
        Ok(record)
    }

    pub async fn delete_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
    ) -> Result<Vec<Message>, MessengerError> {
        let result = self
            .create_messages_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .delete(filter)
            .await?;
        Ok(result)
    }

    pub async fn delete_cursors(
        &self,
        chat_id: &Uuid,
        filter: Document,
    ) -> Result<Vec<CursorRecord>, MessengerError> {
        let result = self
            .create_cursors_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .delete(filter)
            .await?;
        Ok(result)
    }

    pub async fn mark_as_read(
        &self,
        user_id: &str,
        sequence_number: i64,
        chat_id: &Uuid,
    ) -> Result<Option<CursorRecord>, MessengerError> {
        let result = self
            .create_cursors_storage(chat_id)
            .await
            .map_err(MessengerError::StorageError)?
            .update_user_cursor(user_id, sequence_number)
            .await?;

        Ok(result)
    }

    async fn create_messages_storage(
        &self,
        chat_id: &Uuid,
    ) -> Result<Arc<MongoMessageStorage>, mongodb::error::Error> {
        Ok(Arc::new(MongoMessageStorage::new(chat_id).await?))
    }

    async fn create_cursors_storage(
        &self,
        chat_id: &Uuid,
    ) -> Result<Arc<MongoCursorStorage>, mongodb::error::Error> {
        Ok(Arc::new(MongoCursorStorage::new(chat_id).await?))
    }
}
