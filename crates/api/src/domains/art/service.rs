use cortado::{CortadoAffine as ARTGroup, CortadoAffine};
use mongodb::bson::doc;
use storage::{
    ARTStorage, DATABASE, DataStorage, FrameStorage, MongoARTStorage, MongoFramesStorage,
    MongoKeysStorage, StorageError,
};
use tracing::{debug, error};
use types::{ARTRecord, KeyRecord};
use uuid::Uuid;

use types::errors::ARTServiceError;
use types::utils::decode_art;

use mongodb::ClientSession;
use zrt_art::art::PublicArt;
use zrt_art::art_node::TreeMethods;
use zrt_art::changes::ApplicableChange;
use zrt_art::changes::aggregations::AggregatedChange;
use zrt_art::changes::branch_change::{BranchChange, BranchChangeType};

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
        id: Uuid,
        epoch: Option<u64>,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = match epoch {
            Some(epoch) => self.get_art_by_epoch(id, epoch).await.inspect_err(|err| {
                error!(
                    "Failed to get art by id {id} and epoch {epoch}: {}",
                    err.to_string()
                )
            })?,
            None => arts_storage
                .get_art(id)
                .await?
                .ok_or(ARTServiceError::NotFound)
                .inspect_err(|err| {
                    error!("Failed to get latest art by id {}: {}", id, err.to_string());
                })?,
        };

        Ok(record)
    }

    pub async fn get_art_by_epoch(
        &self,
        id: Uuid,
        epoch: u64,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        debug!("Retrieve ART{{ epoch: {}, group: {} }}", epoch, id);

        let frame_storage = MongoFramesStorage::new(&id).await?;

        let mut art_record = self.get_initial_art(&id).await?;
        // Apply other operations from remaining epochs.
        for i in 1..=epoch {
            let epoch_changes = frame_storage.get_epoch_changes(id, i).await?;

            if epoch_changes.is_empty() {
                return Err(ARTServiceError::NotFound);
            }

            epoch_changes.apply(&mut art_record.art)?;

            art_record.art.commit()?;
        }

        art_record.epoch = epoch;
        debug!(
            "Retrieved ART: {{ epoch: {},  group: {}, root PK: {} }}",
            epoch,
            id,
            art_record.art.root().data().public_key()
        );

        Ok(art_record)
    }

    pub async fn get_current_epoch(&self, id: Uuid) -> Result<u64, ARTServiceError> {
        Ok(MongoARTStorage::new().await?.get_current_epoch(&id).await?)
    }

    pub async fn get_initial_art(
        &self,
        chat_id: &Uuid,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = arts_storage
            .get_initial_art(*chat_id)
            .await?
            .ok_or(ARTServiceError::NotFound)?;

        Ok(record)
    }

    pub async fn delete_chat(&self, id: &Uuid) -> Result<(), ARTServiceError> {
        debug!("Deleting group: {}...", id);
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let keys_storage = MongoKeysStorage::new().await?;
        let frame_storage = MongoFramesStorage::new(id).await?;

        if arts_storage.get_art(*id).await?.is_none() {
            error!("No art found for chat: {id}");
            return Err(ARTServiceError::NotFound);
        }

        let mut session = arts_storage
            .arts_collection
            .client()
            .start_session()
            .await?;

        session.start_transaction().await?;

        arts_storage.delete_art(&mut session, *id).await?;
        arts_storage.delete_initial_art(&mut session, *id).await?;
        keys_storage
            .keys_collection
            .delete_one(doc! {"chat_id": id})
            .session(&mut session)
            .await?;

        session.commit_transaction().await?;

        frame_storage.messages_collection.drop().await?;

        debug!("Deletion is successful");

        Ok(())
    }

    pub async fn init_group(
        &self,
        id: Uuid,
        art: Vec<u8>,
        is_private: bool,
        owner_id_pub_key: Vec<u8>,
    ) -> Result<(), ARTServiceError> {
        let art = decode_art(&art)?;

        let arts_storage = MongoARTStorage::new().await?;

        debug!("Check if ART for group {} already exists...", id);
        if arts_storage.get_art(id).await?.is_some() {
            return Err(ARTServiceError::AlreadyExists);
        }
        debug!("Group {} isn't created yet.", id);

        MongoKeysStorage::new()
            .await?
            .insert_one(KeyRecord {
                owner_public_key: owner_id_pub_key,
                chat_id: id,
            })
            .await?;

        let mut session = DATABASE
            .get()
            .ok_or_else(|| StorageError::DatabaseRetrieval)?
            .client()
            .start_session()
            .await?;

        let initial_art_record = ARTRecord {
            chat_id: id,
            art,
            is_private,
            epoch: 0,
        };

        session.start_transaction().await?;
        arts_storage
            .new_group(&mut session, initial_art_record)
            .await?;
        session.commit_transaction().await?;

        debug!("Successfully created new group with id: {id}");

        Ok(())
    }

    pub async fn update_art_tmp(
        &self,
        id: Uuid,
        changes: &BranchChange<ARTGroup>,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;

        let latest_art = arts_storage
            .get_art(id)
            .await?
            .ok_or(ARTServiceError::NotFound)?;
        if latest_art.is_private {
            match changes.change_type {
                BranchChangeType::UpdateKey => {}
                _ => return Err(ARTServiceError::InvalidChangeType),
            }
        }

        let mut session = arts_storage
            .arts_collection
            .client()
            .start_session()
            .await?;
        session.start_transaction().await?;

        self.update_art_in_session(&mut session, changes.clone(), id)
            .await?;

        session.commit_transaction().await?;

        Ok(())
    }

    pub async fn update_art_in_session(
        &self,
        session: &mut ClientSession,
        change: BranchChange<ARTGroup>,
        id: Uuid,
    ) -> Result<(), ARTServiceError> {
        let filter = doc! { "chat_id": id };

        let arts_storage = MongoARTStorage::get_existing_storage().await?;

        debug!("Updating art for chat: {}", id);
        if let Some(mut art_record) = arts_storage
            .arts_collection
            .find_one(filter.clone())
            .await?
        {
            change.apply(&mut art_record.art)?;

            art_record.epoch += 1;

            debug!(
                "Updated art. New root PK is: {}, new epoch is: {}",
                &art_record.art.root().data().public_key(),
                art_record.epoch
            );

            arts_storage.replace_art(session, id, art_record).await?;
        } else {
            error!("Art not found");
            return Err(ARTServiceError::NotFound);
        }

        Ok(())
    }

    pub async fn update_art(
        &self,
        id: Uuid,
        change: &BranchChange<CortadoAffine>,
        new_epoch: u64,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;

        let current_epoch = self.get_current_epoch(id).await?;
        let mut art_record = arts_storage
            .get_art(id)
            .await?
            .ok_or(ARTServiceError::NotFound)?;

        match new_epoch {
            e if e == current_epoch => {
                change.apply(&mut art_record.art)?;
            }
            e if e == current_epoch + 1 => {
                art_record.art.commit()?;
                art_record.epoch += 1;

                change.apply(&mut art_record.art)?;
            }
            _ => return Err(ARTServiceError::InvalidInput.into()),
        }

        let mut session = arts_storage
            .arts_collection
            .client()
            .start_session()
            .await?;
        session.start_transaction().await?;

        arts_storage
            .replace_art(&mut session, id, art_record)
            .await?;

        session.commit_transaction().await?;

        Ok(())
    }

    pub async fn apply_aggregation(
        &self,
        id: Uuid,
        change: AggregatedChange<CortadoAffine>,
        new_epoch: u64,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;

        let mut latest_art = self.get_art_by_epoch(id, new_epoch - 1).await?;

        change.apply(&mut latest_art.art)?;
        latest_art.epoch = new_epoch;

        let mut session = arts_storage
            .arts_collection
            .client()
            .start_session()
            .await?;
        session.start_transaction().await?;

        arts_storage
            .replace_art(&mut session, id, latest_art)
            .await
            .inspect_err(|err| {
                error!(
                    "Failed to replace latest art for group with id: {}. Error: {}",
                    id,
                    err.to_string()
                )
            })?;

        session.commit_transaction().await?;

        Ok(())
    }
}
