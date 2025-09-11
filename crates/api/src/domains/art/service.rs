use ark_ec::AffineRepr;
use ark_serialize::CanonicalDeserialize;
use art::traits::ARTPublicAPI;
use art::types::{BranchChanges, BranchChangesType, PublicART};
use bytes::{BufMut, BytesMut};
use cortado::{CortadoAffine as ARTGroup, CortadoAffine};
use mongodb::bson::{Document, doc};
use prost::Message;
use storage::{
    ARTStorage, DATABASE, DataStorage, FrameStorage, MongoARTStorage, MongoFramesStorage,
    MongoKeysStorage, StorageError,
};
use tracing::{debug, error};
use types::{ARTRecord, FrameRecord, KeyRecord, protos};
use uuid::Uuid;

use types::errors::{ARTServiceError, MessageServiceError};
use types::protos::group_operation::Operation;
use types::utils::{decode_art, decode_branch_changes};

use crate::MessengerService;
#[cfg(not(feature = "art_modifications"))]
use art::types::ARTNode;
use base64::prelude::BASE64_STANDARD;
use futures_util::stream::try_unfold;
use tracing::field::debug;

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
        epoch: Option<u64>,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = match epoch {
            Some(epoch) => self.get_art_by_epoch(chat_id, epoch).await?,
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
        epoch: Option<u64>,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        debug!(
            "Retrieving previous art for sequence_number: {}",
            epoch.unwrap_or(0)
        );
        let arts_storage = MongoARTStorage::new().await?;
        let previous_epoch = arts_storage.get_current_epoch(chat_id).await?;

        let previous_art = match epoch {
            Some(epoch) => {
                if epoch < 1 {
                    error!(
                        "Art sequence_number can't be less than 1, because there is no way to verify the given proof."
                    );
                    return Err(ARTServiceError::NoPreviousRecord);
                }

                if previous_epoch < epoch {
                    error!(
                        "Given sequence_number ({}) is to big (max: {}). There is no way to verify the given proof.",
                        epoch, previous_epoch
                    );
                    return Err(ARTServiceError::NotFound);
                }

                self.get_art_by_epoch(chat_id, epoch - 1).await?
            }
            None => self.get_art_by_epoch(chat_id, previous_epoch - 1).await?,
        };

        Ok(previous_art)
    }

    pub async fn get_art_by_epoch(
        &self,
        chat_id: &Uuid,
        epoch: u64,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let message_storage = MongoFramesStorage::new(chat_id).await?;

        let art_record = self.get_initial_art(chat_id).await?;
        let mut initial_art = art_record.art;

        debug!(
            "Recomputing {} state of the art in the chat: {}",
            epoch, chat_id
        );
        let filter = doc! { "epoch": { "$lte": epoch as i64 } };
        // let sort_options = Some(doc! { "sequence_number": 1 });
        let sort_options = None;

        let mut changes = Vec::with_capacity(epoch as usize);

        let mut skip = 0;
        while changes.len() < epoch as usize {
            let messages = message_storage
                .list(
                    filter.clone(),
                    sort_options.clone(),
                    types::DEFAULT_LIMIT,
                    skip,
                )
                .await?;
            skip += types::DEFAULT_LIMIT;

            if messages.len() == 0 {
                break;
            }

            for message in &messages {
                if let Ok(Some(branch_changes)) =
                    MongoFramesStorage::extract_branch_changes(message)
                {
                    changes.push(branch_changes);
                }
            }
        }

        if changes.len() < epoch as usize {
            error!(
                "Haven't reached state {} in chat {} yet. Current state is {}",
                epoch,
                chat_id,
                changes.len()
            );
            return Err(ARTServiceError::NotFound);
        }

        for change in &changes {
            initial_art.update_public_art(&change)?;
        }

        debug!("Successfully recomputed {} state of art", epoch);
        Ok(ARTRecord {
            chat_id: *chat_id,
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

    pub async fn delete_chat(&self, chat_id: &Uuid) -> Result<(), ARTServiceError> {
        debug!("Deleting chat: {}...", chat_id);
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let keys_storage = MongoKeysStorage::new().await?;
        let frame_storage = MongoFramesStorage::new(chat_id).await?;

        if arts_storage.get_art(*chat_id).await.is_err() {
            error!("No art found for chat: {chat_id}");
            return Err(ARTServiceError::NotFound);
        }

        let mut session = DATABASE
            .get()
            .ok_or_else(|| StorageError::DatabaseRetrieval)?
            .client()
            .start_session()
            .await?;

        session.start_transaction().await?;

        arts_storage.delete_art(&mut session, *chat_id).await?;
        arts_storage
            .delete_initial_art(&mut session, *chat_id)
            .await?;
        keys_storage
            .keys_collection
            .delete_one(doc! {"chat_id": chat_id})
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
        let art = match art.is_empty() {
            false => decode_art(&art)?,
            true => {
                #[cfg(not(feature = "art_modifications"))]
                {
                    debug!("Create group with default art");
                    PublicART {
                        root: Box::new(ARTNode::new_leaf(CortadoAffine::default())),
                        generator: CortadoAffine::default(),
                    }
                }
                #[cfg(feature = "art_modifications")]
                {
                    error!("ART as None is not supported");
                    return Err(ARTServiceError::InvalidInput);
                }
            }
        };

        let arts_storage = MongoARTStorage::new().await?;
        MongoKeysStorage::new()
            .await?
            .insert_one(KeyRecord {
                owner_public_key: owner_id_pub_key,
                chat_id: id,
            })
            .await?;

        debug!("Check if ART for group {} already exists...", id);
        if arts_storage.get_art(id).await.is_ok() {
            return Err(ARTServiceError::AlreadyExists);
        }
        debug!("Group {} isn't created yet.", id);

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
        chat_id: Uuid,
        changes: &BranchChanges<ARTGroup>,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;

        let latest_art = arts_storage.get_art(chat_id).await?;
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

        arts_storage
            .update_art_in_session(&mut session, changes.clone(), chat_id)
            .await?;

        session.commit_transaction().await?;

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

        let mut there_was_add_member = false;
        for applied_change in applied_changes {
            if let BranchChangesType::AppendNode = applied_change.change_type {
                there_was_add_member = true;
                break;
            }
        }

        if let BranchChangesType::AppendNode = &change.change_type {
            if there_was_add_member {
                return Err(ARTServiceError::InvalidChangeType);
            }
        }

        Ok(())
    }

    pub async fn merge_change(
        &self,
        id: Uuid,
        change: &BranchChanges<CortadoAffine>,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let frames_storage = MongoFramesStorage::get_existing_collection(id).await?;

        let latest_art = arts_storage.get_art(id).await?;
        let applied_changes = frames_storage
            .get_epoch_changes(id, latest_art.epoch)
            .await?;

        self.check_if_can_merge(&latest_art, change, &applied_changes)?;

        // let mut session = DATABASE
        //     .get()
        //     .ok_or_else(|| StorageError::DatabaseRetrieval)?
        //     .client()
        //     .start_session()
        //     .await?;

        let mut art_record = self.get_art(&id, None).await?;

        art_record
            .art
            .merge_with_skip(&applied_changes, &vec![change.clone()])?;

        // session.start_transaction().await?;

        arts_storage.replace_art(id, art_record).await?;

        // session.commit_transaction().await?;

        Ok(())
    }
}
