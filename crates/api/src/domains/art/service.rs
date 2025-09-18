use art::traits::ARTPublicAPI;
use art::types::{BranchChanges, BranchChangesType,NodeIndex, PublicART};
use bytes::{BufMut, BytesMut};
use cortado::{CortadoAffine as ARTGroup, CortadoAffine};
use mongodb::bson::doc;
use std::cmp::Ordering;
use std::marker::PhantomData;
use storage::{
    ARTStorage, DATABASE, DataStorage, FrameStorage, MongoARTStorage, MongoFramesStorage,
    MongoKeysStorage, StorageError, KeyStorage, SessionSupport
};
use types::{ARTRecord, KeyRecord, protos, FrameRecord};
use tracing::{debug, error, trace};
use uuid::Uuid;

use types::errors::{ARTServiceError, MessageServiceError};
use types::utils::{decode_art, decode_branch_changes};

use crate::MessengerService;
#[cfg(not(feature = "art_modifications"))]
use art::types::ARTNode;
use mongodb::error::Error;
use prost::Message;
use types::protos::Frame;

pub struct ARTService<A, F, K, S> {
    art_storage_type: PhantomData<A>,
    frame_storage_type: PhantomData<F>,
    key_storage_type: PhantomData<K>,
    session_type: PhantomData<S>,
}

impl<A, F, K, S> ARTService<A, F, K, S> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<A, F, K, S> Default for ARTService<A, F, K, S> {
    fn default() -> Self {
        Self {
            art_storage_type: Default::default(),
            frame_storage_type: Default::default(),
            key_storage_type: Default::default(),
            session_type: Default::default(),
        }
    }
}

impl<A, F, K, S> ARTService<A, F, K, S>
where
    A: ARTStorage<Data = ARTRecord<CortadoAffine>, Session = S> + SessionSupport<A::Error, S>,
    F: FrameStorage<Data = FrameRecord, Error = A::Error, Session = S> + SessionSupport<A::Error, S>,
    K: KeyStorage<Data = KeyRecord, Error = A::Error, Session = S> + SessionSupport<A::Error, S>,
    ARTServiceError: From<A::Error>,
    ARTServiceError: From<F::Error>,
    ARTServiceError: From<K::Error>,
{
    pub async fn get_art(
        &self,
        id: Uuid,
        epoch: Option<u64>,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        trace!("Create ARTStorage");
        let arts_storage = A::new().await?;
        let record = match epoch {
            Some(epoch) => self.get_art_by_epoch(id, epoch).await?,
            None => arts_storage
                .get_art(id)
                .await?
                .ok_or(ARTServiceError::NotFound)?,
        };

        Ok(record)
    }

    pub fn extract_branch_changes(
        messages: &FrameRecord,
    ) -> Result<Frame, ARTServiceError> {
        let mut buf = BytesMut::new();
        buf.put(messages.content.as_slice());
        let frame = Frame::decode(buf)?;

        Ok(frame)
    }

    pub async fn get_all_epoch_changes(&self, id: Uuid, epoch: u64) -> Result<Vec<BranchChanges<CortadoAffine>>, ARTServiceError> {
        let limit = types::DEFAULT_LIMIT;
        let mut skip = 0;

        let mut records = F::new(id)
            .await?
            .list(doc! {"epoch": epoch as i64}, None, limit, skip)
            .await?;

        let mut changes = Vec::new();
        while !records.is_empty() {
            for record in &records {
                if let Some(branch_changes) = types::utils::extract_branch_changes(
                    &Self::extract_branch_changes(record)?
                )? {
                    changes.push(branch_changes);
                }
            }
            skip += types::DEFAULT_LIMIT;

            records = F::new(id)
                .await?
                .list(doc! {"epoch": epoch as i64}, None, limit, skip)
                .await?;

        }

        Ok(changes)
    }

    pub async fn get_art_by_epoch(
        &self,
        id: Uuid,
        epoch: u64,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        debug!("Recomputing {} state of the art in the chat: {}", epoch, id);

        let mut art_record = self.get_initial_art(&id).await?;
        for i in 1..=epoch {
            let epoch_changes = self.get_all_epoch_changes(id, i).await?;

            match epoch_changes.len().cmp(&1) {
                Ordering::Less => return Err(ARTServiceError::NotFound),
                Ordering::Equal => art_record.art.update_public_art(&epoch_changes[0])?,
                Ordering::Greater => art_record.art.merge(&epoch_changes)?,
            }
        }

        art_record.epoch = epoch;
        debug!("Successfully recomputed {} state of art, with PK.x: {}", epoch, art_record.art.root.public_key.x);

        Ok(art_record)
    }

    pub async fn get_art_by_epoch_iterative(
        &self,
        id: &Uuid,
        epoch: u64,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let message_storage = F::new(*id).await?;

        let art_record = self.get_initial_art(id).await?;
        let mut initial_art = art_record.art;

        debug!(
            "Recomputing {} state of the art in the chat: {}",
            epoch, id
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

            if messages.is_empty() {
                break;
            }


            for message in &messages {
                if let Some(branch_changes) = types::utils::extract_branch_changes(
                    &Self::extract_branch_changes(message)?
                )? {
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
        let current_epoch = A::new().await?.get_current_epoch(&id).await?;

        Ok(current_epoch)
    }

    pub async fn get_initial_art(
        &self,
        chat_id: &Uuid,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = A::new().await?;
        match arts_storage.get_initial_art(*chat_id).await? {
            Some(record) => Ok(record),
            None => {
                error!("Failed to retrieve initial art");
                Err(ARTServiceError::NotFound)
            },
        }
    }

    pub async fn delete_chat(&self, id: &Uuid) -> Result<(), ARTServiceError> {
        debug!("Deleting chat: {}...", id);
        let arts_storage = A::new().await?;
        let keys_storage = K::new().await?;
        let frame_storage = F::new(*id).await?;

        if arts_storage.get_art(*id).await?.is_none() {
            error!("No art found for chat: {id}");
            return Err(ARTServiceError::NotFound);
        }

        let mut session = arts_storage.start_session().await?;
        A::start_transaction(&mut session).await?;

        arts_storage.delete_group(&mut session, *id).await?;
        keys_storage
            .delete_one(doc! {"chat_id": id}, Some(&mut session))
            .await?;

        A::commit_transaction(&mut session).await?;

        arts_storage.drop_collection_if_empty().await?;
        frame_storage.drop_collection_if_empty().await?;

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

        let arts_storage = A::new().await?;
        K::new()
            .await?
            .insert_one(
                KeyRecord {
                    owner_public_key: owner_id_pub_key,
                    chat_id: id,
                },
            )
            .await?;

        debug!("Check if ART for group {} already exists...", id);
        if arts_storage.get_art(id).await?.is_some() {
            return Err(ARTServiceError::AlreadyExists);
        }
        debug!("Group {} isn't created yet.", id);

        let mut session = arts_storage.start_session().await?;

        A::start_transaction(&mut session).await?;
        arts_storage
            .new_group(&mut session, art, id, is_private)
            .await?;
        A::commit_transaction(&mut session).await?;

        debug!("Successfully created new group with id: {id}");

        Ok(())
    }

    pub async fn update_art(
        &self,
        chat_id: Uuid,
        changes: &BranchChanges<ARTGroup>,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = A::new().await?;

        let latest_art = arts_storage.get_art(chat_id).await?.ok_or(ARTServiceError::NotFound)?;
        if latest_art.is_private {
            match changes.change_type {
                BranchChangesType::UpdateKey => {}
                _ => return Err(ARTServiceError::InvalidChangeType),
            }
        }

        let mut session = arts_storage.start_session().await?;
        A::start_transaction(&mut session).await?;
        self
            .update_art_in_session(&mut session, changes.clone(), chat_id)
            .await?;
        A::commit_transaction(&mut session).await?;

        Ok(())
    }

    pub async fn mark_as_removed(&self, id: Uuid, index: u64) -> Result<(), ARTServiceError> {
        let arts_storage = A::new().await?;
        
        let mut art_record = arts_storage
            .get_art(id)
            .await?
            .ok_or(ARTServiceError::NotFound)?;
        art_record
            .art
            .get_mut_node(&NodeIndex::from(index))?
            .is_blank = true;

        // art_record.epoch += 1;

        debug!(
            "User with index {index} marked himself as removed",
        );
        arts_storage.replace_art(id, art_record).await?;

        Ok(())
    }

    pub async fn update_art_in_session(
        &self,
        session: &mut S,
        changes: BranchChanges<ARTGroup>,
        chat_id: Uuid,
    ) -> Result<(), ARTServiceError> {
        let filter = doc! { "chat_id": chat_id };

        let arts_storage = A::new().await?;

        debug!("Updating art for chat: {}", chat_id);
        if let Some(mut art_record) = arts_storage.find_one(filter.clone()).await? {
            art_record
                .art
                .update_public_art(&changes)?;

            art_record.epoch += 1;

            debug!(
                "Updated art. New TK_x: {}",
                &art_record.art.root.public_key.x
            );

            arts_storage
                .find_one_and_replace(filter, art_record, Some(&mut *session))
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
        let arts_storage = A::new().await?;

        // let latest_art = arts_storage.get_art(id).await?;
        let mut latest_art = self.get_art_by_epoch(id, new_epoch - 1).await?;
        let mut target_changes = self
            .get_all_epoch_changes(id, new_epoch)
            .await?;

        self.check_if_can_merge(&latest_art, &change, &target_changes)?;


        target_changes.push(change);
        latest_art
            .art
            // .merge_with_skip(&applied_changes, &vec![change.clone()])?;
            .merge(&target_changes)?;

        debug!("Merged art. New TK_x: {}", latest_art.art.root.public_key.x);

        arts_storage.replace_art(id, latest_art).await?;

        Ok(())
    }
}
