use cortado::{CortadoAffine as ARTGroup, CortadoAffine};
use mongodb::bson::doc;
use storage::{
    ARTStorage, DATABASE, DataStorage, FrameStorage, MongoARTStorage, MongoFramesStorage,
    MongoKeysStorage, StorageError,
};
use tracing::{debug, error, warn};
use types::{ARTRecord, KeyRecord};
use uuid::Uuid;

use types::errors::ARTServiceError;
use types::utils::{ArtUpdate, decode_art};

use mongodb::ClientSession;
use proof_verifier::verifier_engine::PostVerificationData;
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
        let mut session = arts_storage
            .arts_collection
            .client()
            .start_session()
            .await?;
        session.start_transaction().await?;

        let record = match epoch {
            Some(epoch) => self
                .get_art_by_epoch(id, epoch, &mut session)
                .await
                .inspect_err(|err| {
                    warn!(
                        "Failed to get art by id {id} and epoch {epoch}: {}",
                        err.to_string()
                    )
                })?,
            None => arts_storage
                .get_current_in_session(id, &mut session)
                .await?
                .ok_or(ARTServiceError::NotFound)
                .inspect_err(|err| {
                    warn!("Failed to get latest art by id {}: {}", id, err.to_string());
                })?,
        };

        session.commit_transaction().await?;
        Ok(record)
    }

    pub async fn get_art_in_session(
        &self,
        id: Uuid,
        epoch: Option<u64>,
        session: &mut ClientSession,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;

        let record = match epoch {
            Some(epoch) => self
                .get_art_by_epoch(id, epoch, &mut *session)
                .await
                .inspect_err(|err| {
                    warn!(
                        "Failed to get art by id {id} and epoch {epoch}: {}",
                        err.to_string()
                    )
                })?,
            None => arts_storage
                .get_current_in_session(id, &mut *session)
                .await?
                .ok_or(ARTServiceError::NotFound)
                .inspect_err(|err| {
                    warn!("Failed to get latest art by id {}: {}", id, err.to_string());
                })?,
        };

        Ok(record)
    }

    pub async fn get_latest_art(
        &self,
        id: Uuid,
    ) -> Result<Option<ARTRecord<ARTGroup>>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = arts_storage.get_current_art(id).await?;

        Ok(record)
    }

    pub async fn get_latest_art_in_session_with_lock(
        &self,
        id: Uuid,
        session: &mut ClientSession,
    ) -> Result<Option<ARTRecord<ARTGroup>>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = arts_storage
            .get_current_art_in_session_in_lock(id, &mut *session)
            .await?;

        Ok(record)
    }

    /// return s the art of provided `epoch`, but uncommited.
    pub async fn get_art_by_epoch(
        &self,
        id: Uuid,
        epoch: u64,
        session: &mut ClientSession,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let frame_storage = MongoFramesStorage::new(&id).await?;

        let mut art_record = self.get_initial_art_in_session(&id, &mut *session).await?;
        // Apply other operations from remaining epochs.
        for i in 1..=epoch {
            let epoch_changes = frame_storage
                .get_epoch_changes_in_session(id, i, &mut *session)
                .await?;

            if epoch_changes.is_empty() {
                return Err(ARTServiceError::NotFound);
            } else {
                art_record.art.commit()?;
            }

            epoch_changes.apply(&mut art_record.art)?;
        }

        art_record.epoch = epoch;
        debug!(
            epoch = ?epoch,
            id = ?id,
            public_key = ?art_record.art.root().data().public_key(),
            public_key_preview = ?art_record.art.preview().root().public_key(),
            "Retrieved ART",
        );

        Ok(art_record)
    }

    pub async fn get_base_art_and_changes(
        &self,
        id: Uuid,
        epoch: u64,
    ) -> Result<(ARTRecord<ARTGroup>, ArtUpdate), ARTServiceError> {
        let frame_storage = MongoFramesStorage::new(&id).await?;

        let mut art_record = self.get_initial_art(&id).await?;
        // Apply other operations from remaining epochs.
        for i in 1..epoch {
            let epoch_changes = frame_storage.get_epoch_changes(id, i).await?;

            if epoch_changes.is_empty() {
                return Err(ARTServiceError::NotFound);
            } else {
                art_record.art.commit()?;
            }

            epoch_changes.apply(&mut art_record.art)?;
        }

        let epoch_changes = if epoch == 0 {
            ArtUpdate::BranchChange(vec![])
        } else {
            let epoch_changes = frame_storage.get_epoch_changes(id, epoch).await?;

            if epoch_changes.is_empty() {
                return Err(ARTServiceError::NotFound);
            } else {
                art_record.art.commit()?;
            }

            epoch_changes
        };

        art_record.epoch = epoch;
        debug!(
            epoch = ?epoch,
            id = ?id,
            public_key = ?art_record.art.root().data().public_key(),
            public_key_preview = ?art_record.art.preview().root().public_key(),
            "Retrieved ART",
        );

        Ok((art_record, epoch_changes))
    }

    pub async fn get_initial_art_in_session(
        &self,
        id: &Uuid,
        session: &mut ClientSession,
    ) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = arts_storage
            .get_initial_art_in_session(*id, session)
            .await
            .inspect_err(|_| error!("Failed to retrieve initial art for group: {}", id))?
            .ok_or(ARTServiceError::NotFound)?;

        Ok(record)
    }

    pub async fn get_initial_art(&self, id: &Uuid) -> Result<ARTRecord<ARTGroup>, ARTServiceError> {
        let arts_storage = MongoARTStorage::new().await?;
        let record = arts_storage
            .get_initial_art(*id)
            .await
            .inspect_err(|_| error!("Failed to retrieve initial art for group: {}", id))?
            .ok_or(ARTServiceError::NotFound)?;

        Ok(record)
    }

    pub async fn delete_chat(
        &self,
        id: &Uuid,
        session: &mut ClientSession,
    ) -> Result<(), ARTServiceError> {
        debug!("Deleting group: {}...", id);
        let arts_storage = MongoARTStorage::get_existing_storage().await?;
        let keys_storage = MongoKeysStorage::new().await?;
        let frame_storage = MongoFramesStorage::new(id).await?;

        if arts_storage
            .get_current_in_session(*id, &mut *session)
            .await?
            .is_none()
        {
            error!("No art found for chat: {id}");
            return Err(ARTServiceError::NotFound);
        }
        arts_storage.delete_art(&mut *session, *id).await?;
        arts_storage.delete_initial_art(&mut *session, *id).await?;
        keys_storage.delete_group(*id, &mut *session).await?;

        frame_storage
            .messages_collection
            .delete_many(doc! {})
            .session(&mut *session)
            .await?;

        debug!("Deletion is successful");

        Ok(())
    }

    pub async fn init_group(
        &self,
        id: Uuid,
        art: Vec<u8>,
        is_private: bool,
        owner_id_pub_key: Vec<u8>,
        session: &mut ClientSession,
    ) -> Result<(), ARTServiceError> {
        let art = decode_art(&art)?;

        let arts_storage = MongoARTStorage::new().await?;

        debug!("Check if ART for group {} already exists...", id);
        if arts_storage
            .get_current_in_session(id, &mut *session)
            .await?
            .is_some()
        {
            return Err(ARTServiceError::AlreadyExists);
        }
        debug!("Group {} isn't created yet.", id);

        MongoKeysStorage::new()
            .await?
            .init_group(owner_id_pub_key, id, &mut *session)
            .await?;

        arts_storage
            .new_group(ARTRecord::new(id, art, is_private), &mut *session)
            .await?;

        MongoFramesStorage::new(&id)
            .await?
            .init_counter(&mut *session)
            .await?;

        debug!(id = ?id, "Successfully created new group");

        Ok(())
    }

    pub async fn update_art(
        &self,
        id: Uuid,
        change: &BranchChange<CortadoAffine>,
        new_epoch: u64,
        post_verification_data: PostVerificationData,
        session: &mut ClientSession,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;

        let mut art_record = arts_storage
            .get_current_art_in_session_in_lock(id, &mut *session)
            .await?
            .ok_or(ARTServiceError::NotFound)?;

        let perform_merge = if new_epoch == art_record.epoch {
            true
        } else if new_epoch == art_record.epoch + 1 {
            false
        } else {
            warn!(
                current_epoch = ?art_record.epoch,
                proposed_epoch = ?new_epoch,
                "Fail to update ART, as the epoch is invalid"
            );
            return Err(ARTServiceError::InvalidInput.into());
        };

        post_verification_data
            .post_verify_update(&art_record)
            .map_err(|_| ARTServiceError::FailedPostVerification)?;

        if perform_merge {
            change.apply(&mut art_record.art)?;
        } else {
            art_record.art.commit()?;
            art_record.epoch += 1;

            change.apply(&mut art_record.art)?;
        }

        debug!(
            epoch = ?art_record.epoch,
            perform_merge = ?perform_merge,
            root_key =? art_record.art.root().data().public_key(),
            root_key_preview =? art_record.art.preview().root().public_key(),
            "Store new art"
        );

        arts_storage
            .replace_art(&mut *session, id, art_record)
            .await?;

        Ok(())
    }

    pub async fn apply_aggregation(
        &self,
        id: Uuid,
        change: AggregatedChange<CortadoAffine>,
        new_epoch: u64,
        session: &mut ClientSession,
    ) -> Result<(), ARTServiceError> {
        let arts_storage = MongoARTStorage::get_existing_storage().await?;

        let mut latest_art = self
            .get_art_by_epoch(id, new_epoch - 1, &mut *session)
            .await?;

        change.apply(&mut latest_art.art)?;
        latest_art.epoch = new_epoch;

        arts_storage
            .replace_art(&mut *session, id, latest_art)
            .await
            .inspect_err(|err| {
                error!(
                    "Failed to replace latest art for group with id: {}. Error: {}",
                    id,
                    err.to_string()
                )
            })?;

        Ok(())
    }
}
