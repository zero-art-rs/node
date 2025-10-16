use crate::domains::{
    art::service::ARTService, centrifugo::service::CentrifugoService,
    messenger::service::MessengerService,
};
use axum::body::Bytes;
use axum::http::StatusCode;
use proof_verifier::ProofVerifierSender;
use prost::Message;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::field::debug;
use tracing::{debug, error};
use types::errors::{ARTServiceError, ServiceError};
use types::protos::Frame;
use types::protos::group_operation::Operation;
use types::utils::decode_branch_changes;
use uuid::Uuid;
use zrt_art::types::BranchChangesType;

const DEFAULT_CHALLENGE_LENGTH: u32 = 16; // 16 bytes

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub art_service: Arc<ARTService>,

    pub proof_verifier_sender: ProofVerifierSender,

    pub challenges: Arc<RwLock<HashSet<Vec<u8>>>>,

    art_is_updating: Arc<RwLock<HashSet<Uuid>>>,
    pub(crate) merge_changes: bool,
}

impl Container {
    pub fn new(
        messenger_service: Arc<MessengerService>,
        centrifugo_service: Arc<CentrifugoService>,
        art_service: Arc<ARTService>,
        proof_verifier_sender: ProofVerifierSender,
        merge_changes: bool,
    ) -> Self {
        Self {
            messenger_service,
            centrifugo_service,
            art_service,
            proof_verifier_sender,
            challenges: Arc::new(RwLock::new(HashSet::new())),
            art_is_updating: Arc::new(RwLock::new(HashSet::new())),
            merge_changes,
        }
    }

    /// Mark ART updating
    pub async fn start_updating(&self, id: Uuid) -> Result<(), ServiceError> {
        // If merge is enabled, there is no management required
        if self.merge_changes {
            return Ok(());
        }

        let mut write_lock = self.art_is_updating.write().await;
        if write_lock.contains(&id) {
            error!("Update failed because another update is in progress.");
            Err(ServiceError::ArtIsUpdating)
        } else {
            debug!("Mark ART in group with id {} as updating.", id);
            write_lock.insert(id);
            Ok(())
        }
    }

    /// Mark ART as not updating.
    pub async fn stop_updating(&self, id: Uuid) {
        // If merge is enabled, then there is no management required
        if self.merge_changes {
            return;
        }

        debug!("Mark ART in group with id {} as free to update.", id);
        self.art_is_updating.write().await.remove(&id);
    }

    /// Handles send_frame operation.
    pub async fn send_frame(&self, id: Uuid, body: Bytes) -> Result<StatusCode, ServiceError> {
        let frame = Frame::decode(body.clone())?;

        let tbs_frame = frame.frame.ok_or_else(|| ARTServiceError::InvalidInput)?;

        let operation = match tbs_frame.group_operation {
            None => None,
            Some(val) => val.operation,
        };

        // Decide, how to handle request
        let response = match operation {
            Some(Operation::Init(public_art)) => {
                self.art_service
                    .init_group(id, public_art, false, tbs_frame.nonce)
                    .await?;

                StatusCode::CREATED
            }
            Some(Operation::AddMember(changes))
            | Some(Operation::RemoveMember(changes))
            | Some(Operation::KeyUpdate(changes))
            | Some(Operation::LeaveGroup(changes)) => {
                self.update_art(id, changes, tbs_frame.epoch).await?
            }
            Some(Operation::DropGroup(_)) => {
                return Ok(self
                    .art_service
                    .delete_chat(&id)
                    .await
                    .map(|_| StatusCode::NO_CONTENT)?);
            }
            Some(Operation::Aggregated(_)) => return Err(ServiceError::NotImplemented),
            None => StatusCode::OK,
        };

        self.messenger_service
            .send_message(body.to_vec(), &id, tbs_frame.epoch as i64, false)
            .await?;

        Ok(response)
    }

    /// Marks the node with `index` as removed.
    pub async fn handle_self_removal(
        &self,
        id: Uuid,
        index: u64,
    ) -> Result<StatusCode, ServiceError> {
        self.art_service.mark_as_removed(id, index).await?;

        Ok(StatusCode::OK)
    }

    pub async fn update_art(
        &self,
        id: Uuid,
        branch_changes_bytes: Vec<u8>,
        new_epoch: u64,
    ) -> Result<StatusCode, ServiceError> {
        let branch_changes =
            decode_branch_changes(&branch_changes_bytes).map_err(ARTServiceError::from)?;

        let current_epoch = self.art_service.get_current_epoch(id).await?;

        match new_epoch {
            e if e == current_epoch => {
                // resolve merge conflict
                self.art_service
                    .merge_change(id, branch_changes.clone(), new_epoch)
                    .await?;
            }
            e if e == current_epoch + 1 => {
                // update art and increment epoch
                self.art_service.update_art(id, &branch_changes).await?;
            }
            _ => return Err(ARTServiceError::InvalidInput.into()),
        }

        match branch_changes.change_type {
            BranchChangesType::UpdateKey => Ok(StatusCode::OK),
            BranchChangesType::AppendNode => Ok(StatusCode::OK),
            BranchChangesType::MakeBlank => Ok(StatusCode::NO_CONTENT),
            BranchChangesType::Leave => Ok(StatusCode::OK),
        }
    }

    // Check if provided correct challenge
    pub async fn contains_challenge(&self, challenge: &Vec<u8>) -> bool {
        let lock = self.challenges.read().await;

        if !lock.contains(challenge) {
            error!("Provided challenge isn't in the state.");
            return false;
        }

        true
    }

    pub async fn new_challenge(&self) -> Vec<u8> {
        let mut lock = self.challenges.write().await;

        let challenge = (0..DEFAULT_CHALLENGE_LENGTH)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<u8>>();

        lock.insert(challenge.clone());

        challenge
    }
}
