use crate::Container;
use art::art::{ART, BranchChanges};
use chrono::Utc;
use message_watcher::MessageWatcher;
use mongodb::bson;
use mongodb::bson::Uuid;
use mongodb::bson::{DateTime, Document, doc, from_document, oid::ObjectId};
use mongodb::change_stream::ChangeStream;
use mongodb::change_stream::event::ChangeStreamEvent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use storage::{
    ARTChangesStorage, ARTStorage, CursorStorage, MessageStorage, MongoARTChangesStorage,
    MongoARTStorage, MongoCursorStorage, MongoMessageStorage,
};
use tokio::sync::mpsc;
use tracing::{debug, error, info};
use types::{ARTChangesRecord, ARTRecord, CursorRecord, Message, Subscription};
use zk::curve::cortado::{CortadoProjective as ARTG, Fr as ScalarField};

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

pub struct MessengerService {
    subscription_sender: mpsc::Sender<Subscription>,
}

impl MessengerService {
    pub fn new(subscription_sender: mpsc::Sender<Subscription>) -> Self {
        Self {
            subscription_sender,
        }
    }
}

impl MessengerService {
    pub async fn subscribe_for_messages(
        &self,
        chat_id: &Uuid,
    ) -> Result<mpsc::Receiver<Message>, MessengerError> {
        let storage = self.create_messages_storage(chat_id).await?;
        let change_stream = storage.stream_messages().await?;

        let (tx, rx) = mpsc::channel(100);

        self.subscription_sender
            .send(Subscription {
                chat_id: chat_id.to_string(),
                change_stream,
                sender: tx,
            })
            .await
            .map_err(|_| {
                MessengerError::StorageError(mongodb::error::Error::custom(
                    "Failed to send subscription to MessageWatcher".to_string(),
                ))
            })?;

        Ok(rx)
    }

    pub async fn send_message(
        &self,
        message: String,
        sender: String,
        chat_id: &Uuid,
    ) -> Result<(), MessengerError> {
        self.create_messages_storage(chat_id)
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
            .create_messages_storage(chat_id)
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
            .create_cursors_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .list_cursors(filter, limit, skip)
            .await?;
        Ok(record)
    }

    pub async fn get_art(
        &self,
        chat_id: &Uuid,
        sequence_number: i64,
    ) -> Result<ARTRecord<ARTG>, MessengerError> {
        let record = self
            .create_arts_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .get_art(sequence_number)
            .await?;

        Ok(record)
    }

    pub async fn find_latest_art(&self, chat_id: &Uuid) -> Result<ARTRecord<ARTG>, MessengerError> {
        let record = self
            .create_arts_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .find_latest_art()
            .await?;

        Ok(record)
    }

    pub async fn list_changes(
        &self,
        chat_id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<ARTChangesRecord<ARTG>>, MessengerError> {
        let record = self
            .create_art_changes_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .list_changes(filter, limit, skip)
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
            .create_cursors_storage(chat_id)
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
            .create_cursors_storage(chat_id)
            .await
            .map_err(|e| MessengerError::StorageError(e))?
            .update_user_cursor(user_id, sequence_number)
            .await?;

        Ok(result)
    }

    pub async fn init_chat(&self, chat_id: &Uuid, art: ART<ARTG>) -> Result<(), MessengerError> {
        let art_storage = self.create_arts_storage(chat_id).await?;

        art_storage.new_art(art).await?;

        Ok(())
    }

    pub async fn update_art(
        &self,
        chat_id: &Uuid,
        changes: BranchChanges<ARTG>,
    ) -> Result<(), MessengerError> {
        self.create_arts_storage(chat_id)
            .await?
            .update_art(changes.clone())
            .await?;

        self.create_art_changes_storage(chat_id)
            .await?
            .store_change(changes)
            .await?;

        Ok(())
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

    async fn create_arts_storage(
        &self,
        chat_id: &Uuid,
    ) -> Result<Arc<MongoARTStorage>, mongodb::error::Error> {
        Ok(Arc::new(MongoARTStorage::new(chat_id).await?))
    }

    async fn create_art_changes_storage(
        &self,
        chat_id: &Uuid,
    ) -> Result<Arc<MongoARTChangesStorage>, mongodb::error::Error> {
        Ok(Arc::new(MongoARTChangesStorage::new(chat_id).await?))
    }
}
