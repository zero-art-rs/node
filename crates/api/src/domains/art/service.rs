use art::types::{BranchChanges, BranchChangesType, PublicART};
use cortado::CortadoAffine as ARTGroup;
use futures_util::FutureExt;
use mongodb::ClientSession;
use mongodb::bson::Document;
use storage::{
    ARTChangesStorage, ARTStorage, DATABASE, DataStorage, MongoARTChangesStorage, MongoARTStorage,
    StorageError,
};
use tracing::error;
use types::{ARTChangesRecord, ARTRecord};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ARTServiceError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("MongoDB error: {0}")]
    MongoDB(#[from] mongodb::error::Error),
    #[error("Unexpected internal error")]
    Internal,
    #[error("Invalid input provided")]
    InvalidInput,
    #[error("Invalid change type provided")]
    InvalidChangeType,
    #[error("Record already exists")]
    AlreadyExists,
    #[error("Record not found")]
    NotFound,
    #[error("The operation can be done only for group chat")]
    GroupChatOnly,
    #[error("Failed to retrieve database")]
    DatabaseRetrieval,
    #[error("Failed to retrieve client")]
    ClientRetrieval,
    #[error("Failed to initiate new session")]
    SessionInitiation,
}

pub struct ARTService {}

impl ARTService {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ARTService {
    fn default() -> Self {
        Self::new()
    }
}

impl ARTService {
    pub async fn get_art(&self, chat_id: &Uuid) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = arts_storage
            .get_art(*chat_id)
            .await
            .map_err(|_| ARTServiceError::NotFound)?;

        Ok(record)
    }

    pub async fn get_initial_art(
        &self,
        chat_id: &Uuid,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = arts_storage
            .get_initial_art(*chat_id)
            .await
            .map_err(|_| ARTServiceError::NotFound)?;

        Ok(record)
    }

    pub async fn delete_chat(&self, chat_id: &Uuid) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;

        if arts_storage.get_art(*chat_id).await.is_err() {
            return Err(ARTServiceError::NotFound);
        }

        let mut session = DATABASE
            .get()
            .ok_or_else(|| StorageError::DatabaseRetrieval)?
            .client()
            .start_session()
            .await?;

        session
            .start_transaction()
            .and_run(chat_id, |session, chat_id| {
                async move { self.delete_chat_callback(session, chat_id).await }.boxed()
            })
            .await
            .map_err(ARTServiceError::MongoDB)?;

        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        arts_storage.drop_collection_if_empty().await?;

        let art_changes_storage = MongoARTChangesStorage::get_existing_collection(chat_id).await?;
        art_changes_storage.drop_collection_if_empty().await?;

        Ok(())
    }

    pub async fn list_changes(
        &self,
        chat_id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<ARTChangesRecord<ARTGroup>>, ARTServiceError> {
        let record = MongoARTChangesStorage::new(chat_id)
            .await?
            .list(filter, limit, skip)
            .await?;
        Ok(record)
    }

    pub async fn init_chat(
        &self,
        chat_id: &Uuid,
        art: PublicART<ARTGroup>,
        is_private: bool,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;

        if arts_storage.get_art(*chat_id).await.is_ok() {
            return Err(ARTServiceError::AlreadyExists);
        }

        arts_storage.new_chat(art, *chat_id, is_private).await?;

        Ok(())
    }

    pub async fn update_art(
        &self,
        chat_id: Uuid,
        changes: &BranchChanges<ARTGroup>,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        if arts_storage.get_art(chat_id).await?.is_private {
            match changes.change_type {
                BranchChangesType::UpdateKey => {}
                _ => return Err(ARTServiceError::InvalidChangeType),
            }
        }

        let mut session = DATABASE
            .get()
            .ok_or_else(|| StorageError::DatabaseRetrieval)?
            .client()
            .start_session()
            .await?;

        session
            .start_transaction()
            .and_run((chat_id, changes), |session, (chat_id, changes)| {
                async move { self.update_art_callback(session, *chat_id, changes).await }.boxed()
            })
            .await
            .map_err(ARTServiceError::MongoDB)?;

        Ok(())
    }

    async fn delete_chat_callback(
        &self,
        session: &mut ClientSession,
        chat_id: &Uuid,
    ) -> mongodb::error::Result<()> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let art_changes_storage = MongoARTChangesStorage::get_existing_collection(chat_id).await?;

        arts_storage.delete_art(session, *chat_id).await?;
        arts_storage.delete_initial_art(session, *chat_id).await?;
        art_changes_storage.clear(session).await?;

        Ok(())
    }

    pub async fn update_art_callback(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
        changes: &BranchChanges<ARTGroup>,
    ) -> Result<(), mongodb::error::Error> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let art_changes_storage = MongoARTChangesStorage::get_existing_collection(&chat_id).await?;

        arts_storage
            .update_art_in_session(session, changes.clone(), chat_id)
            .await?;

        art_changes_storage
            .push_change(session, changes.clone(), chat_id)
            .await?;

        Ok(())
    }
}
