use ark_std::UniformRand;
use art::{ART, BranchChanges, BranchChangesType};
use mongodb::bson;
use mongodb::bson::Document;
use mongodb::bson::Uuid;
use std::sync::Arc;
use storage::{
    ARTChangesStorage, ARTStorage, DataStorage, MongoARTChangesStorage, MongoARTStorage,
};
use tracing::{error, info};
use types::{ARTChangesRecord, ARTRecord};
use zk::curve::cortado::CortadoAffine as ARTGroup;

#[derive(Debug, thiserror::Error)]
pub enum ARTServiceError {
    #[error("Storage error: {0}")]
    StorageError(storage::Error),
    #[error("Conversion error: {0}")]
    ConversionError(bson::oid::Error),
    #[error("Internal error: {0}")]
    InternalError(String),
    #[error("Input error: {0}")]
    InputError(String),
    #[error("Record already exists")]
    AlreadyExists,
}

impl From<storage::Error> for ARTServiceError {
    fn from(error: storage::Error) -> Self {
        ARTServiceError::StorageError(error)
    }
}

pub struct ARTService {}

impl ARTService {
    pub fn new() -> Self {
        Self {}
    }
}

impl ARTService {
    pub async fn get_art(&self, chat_id: &Uuid) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = self
            .create_arts_storage()
            .await
            .map_err(|e| ARTServiceError::StorageError(e))?;
        if !arts_storage.chat_exists(chat_id.clone()).await? {
            return Err(ARTServiceError::InputError(
                "Chat isn't initialized yet.".to_string(),
            ));
        }

        let record = arts_storage.get_art(chat_id.clone()).await?;

        Ok(record)
    }

    pub async fn delete_chat(&self, chat_id: &Uuid) -> Result<(), ARTServiceError> {
        self.create_arts_storage()
            .await
            .map_err(|e| ARTServiceError::StorageError(e))?
            .delete_art(chat_id.clone())
            .await?;

        self.create_art_changes_storage(chat_id)
            .await
            .map_err(|e| ARTServiceError::StorageError(e))?
            .drop_collection()
            .await?;

        Ok(())
    }

    pub async fn list_changes(
        &self,
        chat_id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<ARTChangesRecord<ARTGroup>>, ARTServiceError> {
        let record = self
            .create_art_changes_storage(chat_id)
            .await
            .map_err(|e| ARTServiceError::StorageError(e))?
            .list(filter, limit, skip)
            .await?;
        Ok(record)
    }

    pub async fn init_chat(
        &self,
        chat_id: &Uuid,
        art: ART<ARTGroup>,
        is_private: bool,
    ) -> Result<(), ARTServiceError> {
        let art_storage = self.create_arts_storage().await?;

        if art_storage.chat_exists(chat_id.clone()).await? {
            return Err(ARTServiceError::AlreadyExists);
        }

        art_storage
            .new_chat(art, chat_id.clone(), is_private)
            .await?;

        Ok(())
    }

    pub async fn update_art(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
    ) -> Result<(), ARTServiceError> {
        self.create_arts_storage()
            .await?
            .update_art(changes.clone(), chat_id.clone())
            .await?;

        self.create_art_changes_storage(chat_id)
            .await?
            .push_change(changes.clone())
            .await?;

        Ok(())
    }

    pub async fn add_user(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
    ) -> Result<(), ARTServiceError> {
        match changes.change_type {
            BranchChangesType::AppendNode(_) => {}
            _ => {
                return Err(ARTServiceError::InternalError(
                    "Wrong change type passed. Expected AppendNode change.".to_string(),
                ));
            }
        }

        self.create_arts_storage()
            .await?
            .update_art(changes.clone(), chat_id.clone())
            .await?;

        self.create_art_changes_storage(&chat_id)
            .await?
            .push_change(changes.clone())
            .await?;

        Ok(())
    }

    pub async fn list_chats(&self, limit: i64, skip: i64) -> Result<Vec<Uuid>, ARTServiceError> {
        let arts_storage = self.create_arts_storage().await?;
        arts_storage
            .list_chats(limit, skip)
            .await
            .map_err(|e| ARTServiceError::InternalError(e.to_string()))
    }

    async fn create_arts_storage(&self) -> Result<Arc<MongoARTStorage>, mongodb::error::Error> {
        Ok(Arc::new(MongoARTStorage::new().await?))
    }

    async fn create_art_changes_storage(
        &self,
        chat_id: &Uuid,
    ) -> Result<Arc<MongoARTChangesStorage>, mongodb::error::Error> {
        Ok(Arc::new(MongoARTChangesStorage::new(chat_id).await?))
    }
}
