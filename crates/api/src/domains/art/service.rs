use ark_ec::AffineRepr;
use ark_serialize::CanonicalDeserialize;
use art::traits::ARTPublicAPI;
use art::types::{BranchChanges, BranchChangesType, PublicART};
use bytes::{BufMut, BytesMut};
use cortado::{CortadoAffine as ARTGroup, CortadoAffine};
use mongodb::bson::{Document, doc};
use prost::Message;
use storage::{ARTStorage, DATABASE, DataStorage, FrameStorage, MongoARTStorage, MongoFramesStorage, StorageError, MongoKeysStorage};
use tracing::{debug, error};
use types::{ARTRecord, FrameRecord, protos, KeyRecord};
use uuid::Uuid;

use types::errors::{ARTServiceError, MessageServiceError};
use types::protos::group_operation::Operation;
use types::utils::{decode_art, decode_branch_changes};

#[cfg(not(feature = "art_modifications"))]
use art::types::ARTNode;
use base64::prelude::BASE64_STANDARD;
use tracing::field::debug;

pub const DEFAULT_LIMIT_SIZE: i64 = 10;

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
        epoch: Option<i64>,
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
        epoch: Option<i64>,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        debug!(
            "Retrieving previous art for sequence_number: {}",
            epoch.unwrap_or(0)
        );
        let arts_storage = MongoARTStorage::new().await?;
        let previous_epoch = arts_storage.get_latest_epoch(chat_id).await?;

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
        epoch: i64,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let message_storage = MongoFramesStorage::new(chat_id).await?;

        let art_record = self.get_initial_art(chat_id).await?;
        let mut initial_art = art_record.art;

        debug!(
            "Recomputing {} state of the art in the chat: {}",
            epoch, chat_id
        );
        let filter = doc! { "epoch": { "$lt": epoch } };
        // let sort_options = Some(doc! { "sequence_number": 1 });
        let sort_options = None;

        let mut changes = Vec::with_capacity(epoch as usize);

        let mut skip = 0;
        while changes.len() < epoch as usize {
            let messages = message_storage
                .list(
                    filter.clone(),
                    sort_options.clone(),
                    DEFAULT_LIMIT_SIZE,
                    skip,
                )
                .await?;
            skip += DEFAULT_LIMIT_SIZE;

            if messages.len() == 0 {
                break;
            }

            for message in &messages {
                if let Ok(Some(branch_changes)) = Self::extract_branch_changes(message) {
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

    pub fn extract_branch_changes(
        messages: &FrameRecord,
    ) -> Result<Option<BranchChanges<CortadoAffine>>, ARTServiceError> {
        let mut buf = BytesMut::new();
        buf.put(messages.content.as_slice());
        let frame = protos::Frame::decode(buf)?;

        if let Some(tbs_frame) = &frame.frame {
            if let Some(group_operation) = &tbs_frame.group_operation {
                if let Some(operation) = &group_operation.operation {
                    return match operation {
                        Operation::AddMember(branch_changes) => {
                            Ok(Some(decode_branch_changes(branch_changes)?))
                        }
                        Operation::RemoveMember(branch_changes) => {
                            Ok(Some(decode_branch_changes(branch_changes)?))
                        }
                        Operation::KeyUpdate(branch_changes) => {
                            Ok(Some(decode_branch_changes(branch_changes)?))
                        }
                        _ => Ok(None),
                    };
                }
            }
        }

        Ok(None)
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
        debug!("Delete chat: {}", chat_id);
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
        keys_storage.keys_collection.delete_one(doc! {"chat_id": chat_id}).session(&mut session).await?;

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
            false => {
                decode_art(&art)?
            },
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
        MongoKeysStorage::new().await?.insert_one(KeyRecord {
            owner_public_key: owner_id_pub_key,
            chat_id: id,
        }).await?;

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
        payload: Vec<u8>,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let message_storage = MongoFramesStorage::get_existing_collection(chat_id).await?;

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

        if !payload.is_empty() {
            message_storage
                .store_message_in_session(&mut session, payload, latest_art.epoch + 1)
                .await?;
        }

        session.commit_transaction().await?;

        Ok(())
    }
}
