use cortado::{CortadoAffine as ARTGroup, CortadoAffine};
use mongodb::bson::doc;
use std::cmp::Ordering;
use storage::{
    ARTStorage, DATABASE, DataStorage, FrameStorage, MongoARTStorage, MongoFramesStorage,
    MongoKeysStorage, StorageError,
};
use tracing::{debug, error};
use types::{ARTRecord, KeyRecord};
use uuid::Uuid;
use zrt_art::errors::ARTError;
use zrt_art::traits::{ARTPublicAPI, ARTPublicView};
use zrt_art::types::{BranchChanges, BranchChangesType, LeafStatus, NodeIndex};

use types::errors::ARTServiceError;
use types::utils::decode_art;

use mongodb::ClientSession;

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
            Some(epoch) => self.get_art_by_epoch(id, epoch).await?,
            None => arts_storage
                .get_art(id)
                .await
                .map_err(|_| ARTServiceError::NotFound)?,
        };

        Ok(record)
    }

    pub async fn get_art_by_epoch(
        &self,
        id: Uuid,
        epoch: u64,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        debug!("Recomputing {} state of the art in the chat: {}", epoch, id);

        let frame_storage = MongoFramesStorage::new(&id).await?;

        let mut art_record = self.get_initial_art(&id).await?;

        // Apply other operations from remaining epochs.
        for i in 1..=epoch {
            let epoch_changes = frame_storage.get_epoch_changes(id, i).await?;

            match epoch_changes.len().cmp(&1) {
                Ordering::Less => return Err(ARTServiceError::NotFound),
                Ordering::Equal => art_record.art.update_public_art(&epoch_changes[0])?,
                Ordering::Greater => art_record.art.merge_all(&epoch_changes)?,
            }
        }

        art_record.epoch = epoch;
        debug!(
            "Successfully recomputed {} state of art. It has the next root PK: {}",
            epoch,
            art_record.art.root.get_public_key()
        );

        Ok(art_record)
    }

    pub async fn get_art_by_epoch_iterative(
        &self,
        id: &Uuid,
        epoch: u64,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let message_storage = MongoFramesStorage::new(id).await?;

        let art_record = self.get_initial_art(id).await?;
        let mut initial_art = art_record.art;

        debug!(
            "Recomputing {} state of the art in the group: {}",
            epoch, id
        );
        let filter = doc! { "epoch": { "$lte": epoch as i64 } };

        let mut changes = Vec::with_capacity(epoch as usize);

        let mut skip = 0;
        while changes.len() < epoch as usize {
            let messages = message_storage
                .list(filter.clone(), types::DEFAULT_LIMIT, skip)
                .await?;
            skip += types::DEFAULT_LIMIT;

            if messages.is_empty() {
                break;
            }

            for message in &messages {
                if let Ok(Some(branch_changes)) = MongoFramesStorage::extract_branch_change(message)
                {
                    changes.push(branch_changes);
                }
            }
        }

        if changes.len() < epoch as usize {
            error!(
                "Haven't reached state {} in chat {} yet. Current state is {}",
                epoch,
                id,
                changes.len()
            );
            return Err(ARTServiceError::NotFound);
        }

        for change in &changes {
            initial_art.update_public_art(change)?;
        }

        debug!("Successfully recomputed {} state of art", epoch);
        Ok(ARTRecord {
            chat_id: *id,
            art: initial_art,
            is_private: art_record.is_private,
            epoch,
        })
    }

    pub async fn get_current_epoch(&self, id: Uuid) -> Result<u64, ARTServiceError> {
        let current_epoch = MongoARTStorage::new().await?.get_current_epoch(&id).await?;

        Ok(current_epoch)
    }

    pub async fn get_initial_art(
        &self,
        chat_id: &Uuid,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = arts_storage.get_initial_art(*chat_id).await.map_err(|_| {
            error!("Failed to retrieve initial art");
            ARTServiceError::NotFound
        })?;

        Ok(record)
    }

    pub async fn delete_chat(&self, id: &Uuid) -> Result<(), ARTServiceError> {
        debug!("Deleting group: {}...", id);
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let keys_storage = MongoKeysStorage::new().await?;
        let frame_storage = MongoFramesStorage::new(id).await?;

        if arts_storage.get_art(*id).await.is_err() {
            error!("No art found for chat: {id}");
            return Err(ARTServiceError::NotFound);
        }

        let mut session = DATABASE
            .get()
            .ok_or_else(|| StorageError::DatabaseRetrieval)?
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

        arts_storage.drop_collection_if_empty().await?;
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
        if arts_storage.get_art(id).await.is_ok() {
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

        session.start_transaction().await?;
        arts_storage
            .new_chat(&mut session, art, id, is_private)
            .await?;
        session.commit_transaction().await?;

        debug!("Successfully created new group with id: {id}");

        Ok(())
    }

    pub async fn update_art(
        &self,
        id: Uuid,
        changes: &BranchChanges<ARTGroup>,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;

        let latest_art = arts_storage.get_art(id).await?;
        if latest_art.is_private {
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

        session.start_transaction().await?;

        self.update_art_in_session(&mut session, changes.clone(), id)
            .await?;

        session.commit_transaction().await?;

        Ok(())
    }

    pub async fn mark_as_removed(&self, id: Uuid, index: u64) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let mut art_record = arts_storage.get_art(id).await?;
        art_record
            .art
            .get_mut_node(&NodeIndex::from(index))?
            .set_status(LeafStatus::PendingRemoval)?;

        debug!("User with index {index} marked himself as removed.",);
        arts_storage.replace_art(id, art_record).await?;

        Ok(())
    }

    pub async fn update_art_in_session(
        &self,
        session: &mut ClientSession,
        changes: BranchChanges<ARTGroup>,
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
            art_record.art.update_public_art(&changes)?;

            art_record.epoch += 1;

            debug!(
                "Updated art. New root PK is: {}, new epoch is: {}",
                &art_record.art.root.get_public_key(),
                art_record.epoch
            );

            arts_storage
                .arts_collection
                .find_one_and_replace(filter, art_record)
                .session(session)
                .await?;
        } else {
            error!("Art not found");
            return Err(ARTServiceError::NotFound);
        }

        Ok(())
    }

    pub fn check_if_can_merge(
        &self,
        art_record: &ARTRecord<CortadoAffine>,
        change: &BranchChanges<CortadoAffine>,
        applied_changes: &Vec<BranchChanges<CortadoAffine>>,
    ) -> Result<(), ARTServiceError> {
        if art_record.is_private {
            match change.change_type {
                BranchChangesType::UpdateKey => {}
                _ => return Err(ARTServiceError::InvalidChangeType),
            }
        }

        if let BranchChangesType::AppendNode = &change.change_type {
            let mut there_was_add_member = false;
            for applied_change in applied_changes {
                if let BranchChangesType::AppendNode = applied_change.change_type {
                    there_was_add_member = true;
                    break;
                }
            }

            if there_was_add_member {
                return Err(ARTServiceError::InvalidChangeType);
            }
        }

        Ok(())
    }

    pub async fn merge_change(
        &self,
        id: Uuid,
        change: BranchChanges<CortadoAffine>,
        new_epoch: u64,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let frames_storage = MongoFramesStorage::get_existing_collection(id).await?;

        let mut latest_art = self.get_art_by_epoch(id, new_epoch - 1).await?;
        let mut target_changes =
            frames_storage.get_epoch_changes(id, new_epoch).await?;

        self.check_if_can_merge(&latest_art, &change, &target_changes)?;

        target_changes.push(change);
        latest_art.art.merge_all(&target_changes)?;
        latest_art.epoch = new_epoch;

        debug!(
            "Finished to merge art. New root PK is: {}",
            latest_art.art.root.get_public_key()
        );

        arts_storage.replace_art(id, latest_art).await?;

        Ok(())
    }
}
