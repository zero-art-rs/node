use crate::domains::{
    art::service::ARTService, centrifugo::service::CentrifugoService,
    messenger::service::MessengerService,
};
use crate::verification_middleware;
use crate::verification_middleware::get_current_preview_tk_with_lock;
use axum::body::Bytes;
use axum::http::StatusCode;
use mongodb::ClientSession;
use proof_verifier::ProofVerifierSender;
use proof_verifier::verifier_engine::PostVerificationData;
use prost::Message;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use storage::{ARTStorage, MongoARTStorage, MongoFramesStorage};
use tokio::sync::RwLock;
use tracing::{debug, error, warn};
use types::errors::{ARTServiceError, ServiceError, VerificationError};
use types::protos::Frame;
use types::protos::group_operation::Operation;
use types::utils::{decode_aggregated_change, decode_branch_change, operation_name};
use uuid::Uuid;
use zrt_art::changes::branch_change::BranchChangeType;

const DEFAULT_CHALLENGE_LENGTH: u32 = 16; // 16 bytes

/// How long a challenge stays valid after it was handed out.
const CHALLENGE_TTL: Duration = Duration::from_secs(5 * 60);

/// The most challenges kept at once; the oldest go first. `GET /challenge` needs no
/// authentication, so without a bound every call would grow the node's memory.
const MAX_CHALLENGES: usize = 100_000;

/// Challenges handed out by the node, each valid for `CHALLENGE_TTL`.
#[derive(Default)]
pub struct Challenges {
    issued_at: HashMap<Vec<u8>, Instant>,
    /// the same challenges, oldest first
    order: VecDeque<(Vec<u8>, Instant)>,
}

impl Challenges {
    fn insert(&mut self, challenge: Vec<u8>, now: Instant) {
        // drop expired challenges, and the oldest ones while there is no room
        while self.order.front().is_some_and(|(_, issued_at)| {
            self.order.len() >= MAX_CHALLENGES || now.duration_since(*issued_at) >= CHALLENGE_TTL
        }) {
            if let Some((old, issued_at)) = self.order.pop_front()
                && self.issued_at.get(&old) == Some(&issued_at)
            {
                self.issued_at.remove(&old);
            }
        }
        self.issued_at.insert(challenge.clone(), now);
        self.order.push_back((challenge, now));
    }

    fn contains(&self, challenge: &[u8], now: Instant) -> bool {
        self.issued_at
            .get(challenge)
            .is_some_and(|issued_at| now.duration_since(*issued_at) < CHALLENGE_TTL)
    }
}

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub art_service: Arc<ARTService>,
    pub challenges: Arc<RwLock<Challenges>>,

    pub proof_verifier_sender: ProofVerifierSender,

    // flag, which indicates weather the merges are available
    pub(crate) merge_changes: bool,
    art_is_updating: Arc<RwLock<HashSet<Uuid>>>,

    pub(crate) db_locks: RwLock<HashMap<Uuid, Arc<RwLock<()>>>>,
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
            challenges: Default::default(),
            art_is_updating: Default::default(),
            merge_changes,
            db_locks: Default::default(),
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

        let op_name = frame
            .frame
            .as_ref()
            .and_then(|frame_tbs| frame_tbs.group_operation.as_ref())
            .and_then(|group_operation| group_operation.operation.as_ref())
            .map(operation_name)
            .unwrap_or("None".to_string());
        tracing::Span::current().record("operation", &op_name);

        let tbs_frame = frame.frame.ok_or_else(|| ARTServiceError::InvalidInput)?;
        let post_verification_data =
            verification_middleware::send_frame(self, &tbs_frame, frame.proof.clone(), id).await?;

        let messages_collection = MongoFramesStorage::new(&id).await?;
        let mut session = messages_collection
            .messages_collection
            .client()
            .start_session()
            .await?;

        let group_lock = self.db_locks.write().await.entry(id).or_default().clone();
        let _lock_guard = group_lock.write().await;
        session.start_transaction().await?;

        let operation = tbs_frame
            .group_operation
            .and_then(|group_operation| group_operation.operation.clone());

        let current_epoch = MongoARTStorage::new()
            .await?
            .get_current_epoch_in_session_with_lock(&id, &mut session)
            .await?
            .unwrap_or(0);

        let must_post_verify_epoch = matches!(
            operation,
            None | Some(
                Operation::AddMember(_)
                    | Operation::LeaveGroup(_)
                    | Operation::RemoveMember(_)
                    | Operation::KeyUpdate(_)
                    | Operation::Aggregated(_)
            )
        );

        if must_post_verify_epoch {
            verification_middleware::verify_frame_applicability_by_epoch(
                self,
                operation.as_ref(),
                tbs_frame.epoch,
                current_epoch,
            )
            .await?;
        }

        // Decide, how to handle request
        let response = match &operation {
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
                let post_verification_data = post_verification_data.ok_or(ServiceError::from(
                    VerificationError::FailedPostVerification,
                ))?;
                self.update_art(
                    id,
                    change,
                    tbs_frame.epoch,
                    post_verification_data,
                    &mut session,
                )
                .await?
            }
            Some(Operation::DropGroup(_)) => {
                self.art_service.delete_chat(&id, &mut session).await?;

                StatusCode::NO_CONTENT
            }
            Some(Operation::Aggregated(change)) => {
                let post_verification_data = post_verification_data.ok_or(ServiceError::from(
                    VerificationError::FailedPostVerification,
                ))?;

                self.update_art_with_aggregation(
                    id,
                    change,
                    tbs_frame.epoch,
                    post_verification_data,
                    &mut session,
                )
                .await?
            }
            None => {
                let root_key = get_current_preview_tk_with_lock(self, id, &mut session).await?;

                let post_verification_data = post_verification_data.ok_or(ServiceError::from(
                    VerificationError::FailedPostVerification,
                ))?;

                post_verification_data.post_verify_data_frame(current_epoch, root_key)?;

                StatusCode::OK
            }
        };

        let sequence_number = match &operation {
            Some(Operation::Init(_)) => 0,
            _ => {
                self.messenger_service
                    .next_sequence_number(id, &mut session)
                    .await?
            }
        };

        self.messenger_service
            .send_message(
                body.to_vec(),
                &id,
                tbs_frame.epoch as i64,
                sequence_number,
                false,
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
        post_verification_data: PostVerificationData,
        session: &mut ClientSession,
    ) -> Result<StatusCode, ServiceError> {
        let change = decode_branch_change(change_bytes).map_err(ARTServiceError::from)?;

        self.art_service
            .update_art(id, &change, new_epoch, post_verification_data, session)
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
        aggregation_bytes: &[u8],
        new_epoch: u64,
        post_verification_data: PostVerificationData,
        session: &mut ClientSession,
    ) -> Result<StatusCode, ServiceError> {
        let branch_changes =
            decode_aggregated_change(aggregation_bytes).map_err(ARTServiceError::from)?;

        self.art_service
            .update_art(
                id,
                &branch_changes,
                new_epoch,
                post_verification_data,
                session,
            )
            .await?;

        Ok(StatusCode::OK)
    }

    // Check if provided correct challenge
    pub async fn contains_challenge(&self, challenge: &Vec<u8>) -> bool {
        let lock = self.challenges.read().await;

        if !lock.contains(challenge, Instant::now()) {
            error!("Provided challenge isn't in the state or has expired.");
            return false;
        }

        true
    }

    pub async fn new_challenge(&self) -> Vec<u8> {
        let mut lock = self.challenges.write().await;

        let challenge = (0..DEFAULT_CHALLENGE_LENGTH)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<u8>>();

        lock.insert(challenge.clone(), Instant::now());

        challenge
    }
}

#[cfg(test)]
mod challenge_tests {
    use super::*;

    #[test]
    fn challenges_expire() {
        let start = Instant::now();
        let mut challenges = Challenges::default();
        challenges.insert(vec![1], start);
        assert!(challenges.contains(&[1], start + CHALLENGE_TTL - Duration::from_secs(1)));
        assert!(!challenges.contains(&[1], start + CHALLENGE_TTL));
        assert!(!challenges.contains(&[2], start));

        // expired challenges are dropped when a new one is handed out
        challenges.insert(vec![2], start + CHALLENGE_TTL);
        assert_eq!(challenges.issued_at.len(), 1);
        assert_eq!(challenges.order.len(), 1);
    }

    #[test]
    fn challenges_are_bounded() {
        let now = Instant::now();
        let mut challenges = Challenges::default();
        for i in 0..MAX_CHALLENGES + 10 {
            challenges.insert((i as u64).to_le_bytes().to_vec(), now);
        }
        assert_eq!(challenges.issued_at.len(), MAX_CHALLENGES);
        assert_eq!(challenges.order.len(), MAX_CHALLENGES);
        // the oldest went first
        assert!(!challenges.contains(&0u64.to_le_bytes(), now));
        assert!(challenges.contains(&((MAX_CHALLENGES + 9) as u64).to_le_bytes(), now));
    }
}
