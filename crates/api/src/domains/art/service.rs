use ark_std::iterable::Iterable;
use art::errors::ARTError;
use art::traits::ARTPublicAPI;
use art::types::{BranchChanges, BranchChangesType, PublicART};
use cortado::CortadoAffine as ARTGroup;
use futures_util::FutureExt;
use mongodb::ClientSession;
use mongodb::bson::{Document, doc};
use storage::{
    ARTChangesStorage, ARTStorage, DATABASE, DataStorage, MongoARTChangesStorage, MongoARTStorage,
    StorageError,
};
use tracing::error;
use types::{ARTChangesRecord, ARTRecord, ProofRecord};
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
    #[error("Failed to use ART {0}")]
    ArtError(#[from] ARTError),
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
    pub async fn get_art(
        &self,
        chat_id: &Uuid,
        sequence_number: Option<i64>,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = match &sequence_number {
            Some(sequence_number) => {
                self.get_art_by_sequence_number(chat_id, *sequence_number)
                    .await?
            }
            None => arts_storage
                .get_art(*chat_id)
                .await
                .map_err(|_| ARTServiceError::NotFound)?,
        };

        Ok(record)
    }

    pub async fn get_previous_art(
        &self,
        chat_id: &Uuid,
        sequence_number: Option<i64>,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let latest_sequence_number = arts_storage.get_latest_sequence_number(chat_id).await?;

        let previous_art = match sequence_number {
            Some(sequence_number) => {
                if sequence_number < 1 || latest_sequence_number < sequence_number {
                    return Err(ARTServiceError::InvalidInput);
                }

                self.get_art_by_sequence_number(chat_id, sequence_number - 1)
                    .await?
            }
            None => {
                self.get_art_by_sequence_number(chat_id, latest_sequence_number - 1)
                    .await?
            }
        };

        Ok(previous_art)
    }

    pub async fn get_art_by_sequence_number(
        &self,
        chat_id: &Uuid,
        sequence_number: i64,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let art_record = self.get_initial_art(chat_id).await?;
        let mut initial_art = art_record.art;

        let filter = doc! { "sequence_number": { "$lt": sequence_number } };
        let mut changes = self
            .list_changes(chat_id, filter, sequence_number, 0)
            .await?;

        if changes.len() < sequence_number as usize {
            return Err(ARTServiceError::NotFound);
        }

        changes.sort_by(|a, b| a.sequence_number.cmp(&b.sequence_number));

        for change in &changes {
            initial_art.update_public_art(&change.change)?;
        }

        Ok(ARTRecord {
            chat_id: *chat_id,
            art: initial_art,
            is_private: art_record.is_private,
            sequence_number,
        })
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

    pub async fn update_key(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
        proof_record: &ProofRecord,
    ) -> Result<(), ARTServiceError> {
        match changes.change_type {
            BranchChangesType::UpdateKey => self.update_art(chat_id, changes, proof_record).await,
            _ => Err(ARTServiceError::InvalidChangeType),
        }
    }

    pub async fn append_member(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
        proof_record: &ProofRecord,
    ) -> Result<(), ARTServiceError> {
        match changes.change_type {
            BranchChangesType::AppendNode(_) => {
                self.update_art(chat_id, changes, proof_record).await
            }
            _ => Err(ARTServiceError::InvalidChangeType),
        }
    }

    pub async fn remove_member(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
        proof_record: &ProofRecord,
    ) -> Result<(), ARTServiceError> {
        match changes.change_type {
            BranchChangesType::MakeBlank(_, _) => {
                self.update_art(chat_id, changes, proof_record).await
            }
            _ => Err(ARTServiceError::InvalidChangeType),
        }
    }

    async fn update_art(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
        proof_record: &ProofRecord,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        if arts_storage.get_art(*chat_id).await?.is_private {
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
            .and_run(
                (chat_id, changes, proof_record),
                |session, (chat_id, changes, proof_record)| {
                    async move {
                        self.update_art_callback(session, chat_id, changes, proof_record)
                            .await
                    }
                    .boxed()
                },
            )
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
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
        proof_record: &ProofRecord,
    ) -> Result<(), mongodb::error::Error> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let art_changes_storage = MongoARTChangesStorage::get_existing_collection(chat_id).await?;

        arts_storage
            .update_art_in_session(session, changes.clone(), *chat_id)
            .await?;

        art_changes_storage
            .push_change(session, changes.clone(), proof_record.clone())
            .await?;

        Ok(())
    }
}
