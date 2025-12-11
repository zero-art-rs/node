use crate::domains::{
    art::service::ARTService, centrifugo::service::CentrifugoService,
    messenger::service::MessengerService,
};
use crate::verification_middleware;
use axum::body::Bytes;
use axum::http::StatusCode;
use mongodb::ClientSession;
use mongodb::bson::doc;
use proof_verifier::ProofVerifierSender;
use prost::Message;
use std::collections::HashSet;
use std::sync::Arc;
use mongodb::atlas_search::autocomplete;
use storage::{ARTStorage, MongoARTStorage, MongoFramesStorage};
use tokio::sync::{Mutex, RwLock};
use tracing::field::debug;
use tracing::{debug, error, warn};
use types::errors::{ARTServiceError, ServiceError, VerificationError};
use types::protos::Frame;
use types::protos::group_operation::Operation;
use types::utils::{decode_aggregated_change, decode_branch_change};
use uuid::Uuid;
use zrt_art::changes::branch_change::BranchChangeType;

const DEFAULT_CHALLENGE_LENGTH: u32 = 16; // 16 bytes

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub art_service: Arc<ARTService>,

    pub proof_verifier_sender: ProofVerifierSender,

    pub challenges: Arc<RwLock<HashSet<Vec<u8>>>>,

    art_is_updating: Arc<RwLock<HashSet<Uuid>>>,
    // flag, which indicates weather the merges are available
    pub(crate) merge_changes: bool,

    pub update_mutex: Arc<Mutex<bool>>,
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
            update_mutex: Arc::new(Default::default()),
        }
    }

    /// Mark ART updating
    pub async fn start_updating(&self, id: Uuid) -> Result<(), VerificationError> {
        // If merge is enabled, there is no management required
        if self.merge_changes {
            return Ok(());
        }

        let mut write_lock = self.art_is_updating.write().await;
        if write_lock.contains(&id) {
            error!("Update failed because another update is in progress.");
            Err(VerificationError::ArtIsUpdating)
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

        let messages_collection = MongoFramesStorage::new(&id).await?;
        let mut session = messages_collection
            .messages_collection
            .client()
            .start_session()
            .await?;
        session.start_transaction().await?;

        verification_middleware::send_frame(self, &tbs_frame, frame.proof, id)
            .await?;

        session.commit_transaction().await?;
        let mut session = messages_collection
            .messages_collection
            .client()
            .start_session()
            .await?;
        session.start_transaction().await?;


        let operation = match &tbs_frame.group_operation {
            None => None,
            Some(val) => val.operation.clone(),
        };

        let current_epoch = MongoARTStorage::new()
            .await?
            .get_current_epoch_in_session_with_lock(&id, &mut session)
            .await?
            .unwrap_or(0);

        if matches!(operation, None | Some(
            Operation::AddMember(_)
            | Operation::LeaveGroup(_)
            | Operation::RemoveMember(_)
            | Operation::KeyUpdate(_)
            | Operation::Aggregated(_)
        )) {
            verification_middleware::verify_frame_applicability_by_epoch(
                self,
                id,
                operation.as_ref(),
                tbs_frame.epoch,
                current_epoch,
            ).await?;
        }

        // Decide, how to handle request
        let response = match operation.clone() {
            Some(Operation::Init(public_art)) => {
                self.art_service
                    .init_group(id, public_art, false, tbs_frame.nonce, &mut session)
                    .await?;

                StatusCode::CREATED
            }
            Some(Operation::AddMember(change))
            | Some(Operation::RemoveMember(change))
            | Some(Operation::KeyUpdate(change))
            | Some(Operation::LeaveGroup(change)) => {
                self.update_art(id, &change, tbs_frame.epoch, &mut session)
                    .await?
            }
            Some(Operation::DropGroup(_)) => {
                self.art_service.delete_chat(&id, &mut session).await?;

                StatusCode::NO_CONTENT
            }
            Some(Operation::Aggregated(change)) => {
                self.update_art_with_aggregation(id, change, tbs_frame.epoch, &mut session)
                    .await?
            }
            None => StatusCode::OK,
        };

        let sequence_number = if let Some(Operation::Init(_)) = &operation {
            0
        } else {
            self.messenger_service.next_sequence_number(id, &mut session).await?
        };

        self.messenger_service
            .send_message(
                body.to_vec(),
                &id,
                tbs_frame.epoch as i64,
                sequence_number,
                false,
                operation,
                &mut session,
            )
            .await?;

        session.commit_transaction().await.inspect_err(|err| {
            warn!("Failed to commit transaction: {}", err);
        })?;

        Ok(response)
    }

    pub async fn update_art(
        &self,
        id: Uuid,
        change_bytes: &[u8],
        new_epoch: u64,
        session: &mut ClientSession,
    ) -> Result<StatusCode, ServiceError> {
        let change = decode_branch_change(change_bytes).map_err(ARTServiceError::from)?;

        self.art_service
            .update_art(id, &change, new_epoch, session)
            .await?;

        match change.change_type {
            BranchChangeType::UpdateKey => Ok(StatusCode::OK),
            BranchChangeType::AddMember => Ok(StatusCode::OK),
            BranchChangeType::RemoveMember => Ok(StatusCode::NO_CONTENT),
            BranchChangeType::Leave => Ok(StatusCode::OK),
        }
    }

    pub async fn update_art_with_aggregation(
        &self,
        id: Uuid,
        aggregated_change_bytes: Vec<u8>,
        new_epoch: u64,
        session: &mut ClientSession,
    ) -> Result<StatusCode, ServiceError> {
        let branch_changes =
            decode_aggregated_change(&aggregated_change_bytes).map_err(ARTServiceError::from)?;

        self.art_service
            .apply_aggregation(id, branch_changes.clone(), new_epoch, session)
            .await?;

        Ok(StatusCode::OK)
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
