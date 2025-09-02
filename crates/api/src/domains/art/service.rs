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
use tracing::{debug, error};
use types::{ARTChangesRecord, ARTRecord};
use uuid::Uuid;

use types::errors::ARTServiceError;

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
        debug!(
            "Retrieving previous art for sequence_number: {}",
            sequence_number.unwrap_or(0)
        );
        let arts_storage = MongoARTStorage::new().await?;
        let latest_sequence_number = arts_storage.get_latest_sequence_number(chat_id).await?;

        let previous_art = match sequence_number {
            Some(sequence_number) => {
                if sequence_number < 1 {
                    error!(
                        "Art sequence_number can't be less than 1, because there is no way to verify the given proof."
                    );
                    return Err(ARTServiceError::NoPreviousRecord);
                }

                if latest_sequence_number < sequence_number {
                    error!(
                        "Given sequence_number ({}) is to big (max: {}). There is no way to verify the given proof.",
                        sequence_number, latest_sequence_number
                    );
                    return Err(ARTServiceError::NotFound);
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

        debug!(
            "Recomputing {} state of the art in the chat: {}",
            sequence_number, chat_id
        );
        let filter = doc! { "sequence_number": { "$lt": sequence_number } };
        let mut changes = self
            .list_changes(chat_id, filter, sequence_number, 0)
            .await?;

        if changes.len() < sequence_number as usize {
            error!(
                "Haven't reached state {} in chat {} yet",
                sequence_number, chat_id
            );
            return Err(ARTServiceError::NotFound);
        }

        changes.sort_by(|a, b| a.sequence_number.cmp(&b.sequence_number));

        for change in &changes {
            initial_art.update_public_art(&change.changes)?;
        }

        debug!("Successfully recomputed {} state of art", sequence_number);
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
        let record = arts_storage.get_initial_art(*chat_id).await.map_err(|_| {
            error!("Failed to retreive initial art");
            ARTServiceError::NotFound
        })?;

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

        debug!("Check if ART for chat {} already exists", chat_id);
        if arts_storage.get_art(*chat_id).await.is_ok() {
            return Err(ARTServiceError::AlreadyExists);
        }

        debug!("Chat {} isn't created yet.", chat_id);

        let mut session = DATABASE
            .get()
            .ok_or_else(|| StorageError::DatabaseRetrieval)?
            .client()
            .start_session()
            .await?;

        session.start_transaction().await?;
        arts_storage
            .new_chat(&mut session, art, *chat_id, is_private)
            .await?;
        session.commit_transaction().await?;

        Ok(())
    }

    pub async fn update_key(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
        metadata: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        proof: &Vec<u8>,
    ) -> Result<(), ARTServiceError> {
        match changes.change_type {
            BranchChangesType::UpdateKey => {
                self.update_art(chat_id, changes, metadata, payload, proof)
                    .await
            }
            _ => Err(ARTServiceError::InvalidChangeType),
        }
    }

    pub async fn append_member(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
        proof: &Vec<u8>,
    ) -> Result<(), ARTServiceError> {
        match changes.change_type {
            BranchChangesType::AppendNode => {
                self.update_art(chat_id, changes, None, None, proof).await
            }
            _ => Err(ARTServiceError::InvalidChangeType),
        }
    }

    pub async fn remove_member(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
        proof: &Vec<u8>,
    ) -> Result<(), ARTServiceError> {
        match changes.change_type {
            BranchChangesType::MakeBlank => {
                self.update_art(chat_id, changes, None, None, proof).await
            }
            _ => Err(ARTServiceError::InvalidChangeType),
        }
    }

    pub async fn update_metadata(
        &self,
        chat_id: Uuid,
        new_metadata: Vec<u8>,
        node_index: i64,
    ) -> Result<(), ARTServiceError> {
        MongoARTStorage::new()
            .await?
            .update_metadata(chat_id, new_metadata, node_index)
            .await?;

        Ok(())
    }

    pub async fn update_art(
        &self,
        chat_id: &Uuid,
        changes: &BranchChanges<ARTGroup>,
        metadata: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        proof: &Vec<u8>,
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
                (chat_id, changes, metadata, payload, proof),
                |session, (chat_id, changes, metadata, payload, proof)| {
                    async move {
                        self.update_art_callback(
                            session,
                            chat_id,
                            changes,
                            metadata.clone(),
                            payload.clone(),
                            proof,
                        )
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
        metadata: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        proof: &Vec<u8>,
    ) -> Result<(), mongodb::error::Error> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let art_changes_storage = MongoARTChangesStorage::get_existing_collection(chat_id).await?;

        arts_storage
            .update_art_in_session(session, changes.clone(), *chat_id, metadata.clone())
            .await?;

        art_changes_storage
            .push_change(
                session,
                changes.clone(),
                *chat_id,
                metadata,
                payload,
                proof.clone(),
            )
            .await?;

        Ok(())
    }
}
