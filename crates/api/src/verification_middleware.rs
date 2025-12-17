use crate::Container;
use ark_serialize::CanonicalDeserialize;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum_core::body::Body;
use axum_core::extract::{FromRequestParts, Request};
use axum_core::response::Response;
use bytes::Bytes;
use callbacks::callback;
use cortado::CortadoAffine;
use mongodb::ClientSession;
use proof_verifier::ProofVerifierSender;
use proof_verifier::verifier_engine::*;
use prost::Message;
use sha3::{Digest, Sha3_256};
use std::sync::Arc;
use storage::{ARTStorage, FrameStorage, MongoARTStorage, MongoFramesStorage};
use tracing::{Level, debug, error, info, trace, warn};
use types::art_schemas::{GetARTQuery, ProofMode};
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use types::centrifugo_schemas::AuthRequest;
use types::errors::{ARTServiceError, VerificationError};
use types::messenger_schemas::GetMessageQuery;
use types::protos::{Frame, FrameTbs, group_operation::Operation};
use types::utils::ArtUpdate;
use uuid::Uuid;
use zrt_art::art::PublicArt;
use zrt_art::art_node::{LeafIter, LeafStatus, TreeMethods};
use zrt_art::changes::ApplicableChange;
use zrt_art::changes::aggregations::AggregatedChange;
use zrt_art::changes::branch_change::{BranchChange, BranchChangeType};
use zrt_art::node_index::{Direction, NodeIndex};
use zrt_zk::EligibilityRequirement;

/// Handle authentication request verification.
pub async fn authenticate(
    State(state): State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Result<Response, VerificationError> {
    debug!(
        "Incoming authenticate verification request: {} {}.",
        request.method(),
        request.uri(),
    );

    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await?;
    let Json(auth_request) = Json::<AuthRequest>::from_bytes(&bytes)?;

    if !state.contains_challenge(&auth_request.challenge).await {
        error!("Challenge not found in the current state");
        return Err(VerificationError::WrongChallenge);
    }

    if auth_request.chat_ids.len() != auth_request.epochs.len() {
        error!(
            "The len of ids and epochs must be the same, but provided {} ids and {} epochs.",
            auth_request.chat_ids.len(),
            auth_request.epochs.len()
        );
        return Err(VerificationError::InvalidInput);
    }

    let mut root_keys = Vec::new();
    for (chat_id, epoch) in auth_request.chat_ids.iter().zip(auth_request.epochs.iter()) {
        let art = state.art_service.get_art(*chat_id, Some(*epoch)).await?.art;

        root_keys.push(art.root().data().public_key());
    }

    let verification_req = VerificationRequest {
        opcode: VerificationOpcode::AuthRequest,
        data: VerifierData {
            proof: auth_request.proof,
            public_inputs: PublicInputs::Signature {
                public_keys: root_keys,
            },
            associated_data: Sha3_256::digest(auth_request.challenge).to_vec(),
        },
    };

    verify(verification_req.to_message()?, &state.proof_verifier_sender).await?;

    Ok(next
        .run(Request::from_parts(
            parts.clone(),
            Body::from(bytes.clone()),
        ))
        .await)
}

/// Handle list messages request verification
pub async fn list_messages(
    State(state): State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Result<Response, VerificationError> {
    debug!(
        "Incoming verification request: {} {}.",
        request.method(),
        request.uri(),
    );

    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await?;

    let query = parts.uri.query();
    let query_bytes = query.ok_or(VerificationError::MissingQuery)?.as_bytes();

    let Path(chat_id) = Path::<Uuid>::from_request_parts(&mut parts.clone(), &state).await?;
    let payload = serde_urlencoded::from_bytes::<GetMessageQuery>(query_bytes)?;

    // let art = state
    //     .art_service
    //     .get_art(chat_id, Some(payload.epoch.unwrap_or(0)))
    //     .await?
    //     .art;

    let (art, art_change) = state
        .art_service
        .get_base_art_and_changes(chat_id, payload.epoch.unwrap_or(0))
        .await?;
    let mut art = art.art;

    let mut partial_tree_keys = Vec::new();
    partial_tree_keys.push(art.root().data().public_key());
    match art_change {
        ArtUpdate::BranchChange(changes) => {
            for change in changes {
                change.apply(&mut art)?;
                partial_tree_keys.push(art.preview().root().public_key())
            }
        }
        ArtUpdate::AggregatedChange(aggregation) => {
            aggregation.apply(&mut art)?;
            partial_tree_keys.push(art.preview().root().public_key())
        }
    }

    let mut msg = Vec::new();
    msg.extend_from_slice(chat_id.as_bytes());
    msg.extend(&payload.nonce);
    let msg = Sha3_256::digest(&msg).to_vec();

    let mut verified = false;
    for root_key in partial_tree_keys {
        let verification_req = VerificationRequest {
            opcode: VerificationOpcode::GetMessages,
            data: VerifierData {
                proof: payload.signature.clone(),
                public_inputs: PublicInputs::Signature {
                    public_keys: vec![root_key],
                },
                associated_data: msg.clone(),
            },
        };

        if verdict(verification_req.to_message()?, &state.proof_verifier_sender).await? {
            verified = true;
            break;
        }
    }

    if !verified {
        return Err(VerificationError::InvalidProof);
    }

    Ok(next
        .run(Request::from_parts(
            parts.clone(),
            Body::from(bytes.clone()),
        ))
        .await)
}

/// Handle get_art request verification.
pub async fn get_art(
    State(state): State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Result<Response, VerificationError> {
    debug!(
        "Incoming verification for get art request: {} {}.",
        request.method(),
        request.uri(),
    );

    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await?;

    let Path((chat_id, epoch)) =
        Path::<(Uuid, u64)>::from_request_parts(&mut parts.clone(), &state).await?;

    let query_bytes = parts
        .uri
        .query()
        .ok_or(VerificationError::MissingQuery)?
        .as_bytes();
    let payload = serde_urlencoded::from_bytes::<GetARTQuery>(query_bytes)?;

    let public_key = CortadoAffine::deserialize_compressed(&*payload.public_key)?;

    let art = state.art_service.get_art(chat_id, Some(epoch)).await?.art;

    match ProofMode::try_from(payload.proof_mode.as_str())? {
        ProofMode::UseLeafKey => {
            let mut public_key_is_wrong = true;
            for node in LeafIter::new(art.root()) {
                if node.data().public_key().eq(&public_key) {
                    public_key_is_wrong = false;
                }
            }

            if public_key_is_wrong {
                error!("Provided leaf public key doesn't match any leaf in the tree.");
                return Err(VerificationError::InvalidInput);
            }
        }
        ProofMode::UseRootKey => {
            if art.root().data().public_key() != public_key {
                error!("Provided public key doesn't match with root key.");
                return Err(VerificationError::InvalidInput);
            }
        }
    }

    if !state.contains_challenge(&payload.challenge).await {
        error!("Challenge not found in the current state");
        return Err(VerificationError::WrongChallenge);
    }

    // Context for verification
    let mut msg = Vec::new();
    msg.extend_from_slice(chat_id.as_bytes());
    msg.extend(&payload.nonce);
    msg.extend(payload.challenge);
    msg.extend(epoch.to_be_bytes());
    let msg = Sha3_256::digest(&msg).to_vec();

    let verification_req = VerificationRequest {
        opcode: VerificationOpcode::GetArt,
        data: VerifierData {
            proof: payload.signature.clone(),
            public_inputs: PublicInputs::Signature {
                public_keys: vec![public_key],
            },
            associated_data: msg,
        },
    };

    verify(verification_req.to_message()?, &state.proof_verifier_sender).await?;

    Ok(next
        .run(Request::from_parts(
            parts.clone(),
            Body::from(bytes.clone()),
        ))
        .await)
}

pub async fn send_frame(
    state: &Container,
    frame_tbs: &FrameTbs,
    proof: Vec<u8>,
    id: Uuid,
) -> Result<Option<PostVerificationData>, VerificationError> {
    let current_epoch = MongoARTStorage::new()
        .await?
        .get_current_epoch(&id)
        .await?
        .unwrap_or(0);

    state.start_updating(id).await?;

    let (verification_req, post_verification_data) =
        match inner_send_frame(state, current_epoch, id, frame_tbs, proof).await {
            Ok(verification_result) => verification_result,
            Err(err) => {
                warn!("Verification Failed: {err}");
                state.stop_updating(id).await;
                return Err(err);
            }
        };

    let response = verify_and_send(verification_req, state).await;

    state.stop_updating(id).await;

    response?;
    Ok(post_verification_data)
}

/// Handle send_frame verification
async fn inner_send_frame(
    state: &Container,
    current_epoch: u64,
    id: Uuid,
    frame_tbs: &FrameTbs,
    proof: Vec<u8>,
) -> Result<(VerificationRequest, Option<PostVerificationData>), VerificationError> {
    if let Some(art) = state.art_service.get_latest_art(id).await? {
        info!(
            current_epoch = ?current_epoch,
            record_epoch = ?art.epoch,
            public_key = ?art.art.root().data().public_key(),
            public_key_preview = ?art.art.preview().root().public_key(),
            group_id = ?id,
            "Start verification",
        );
    } else {
        debug!("Start verification: No art found for epoch {}", current_epoch);
    }

    if frame_tbs.group_id != id.to_string() {
        error!(
            "Group ID mismatch: tbs_frame.group_id is {}, while id in path is {}",
            frame_tbs.group_id, id
        );
        return Err(VerificationError::InvalidInput);
    }

    let associated_data = Sha3_256::digest(frame_tbs.encode_to_vec()).to_vec();

    let operation = frame_tbs
        .group_operation
        .as_ref()
        .and_then(|op| op.operation.as_ref());

    verify_frame_applicability_by_epoch(state, id, operation, frame_tbs.epoch, current_epoch)
        .await
        .inspect_err(|err| {
            error!(
                error = ?err,
                group_id = ?id,
                provided_epoch = ?frame_tbs.epoch,
                current_epoch = ?current_epoch,
                "Error verifying frame applicability by epoch"
            )
        })?;

    let is_current_epoch = frame_tbs.epoch == current_epoch;
    let (opcode, public_inputs, post_verification_data) = match &operation {
        Some(Operation::Init(_)) => {
            let (opcode, public_inputs) = get_opcode_and_input_for_init_group(&frame_tbs)?;
            (opcode, public_inputs, None)
        }
        Some(Operation::AddMember(branch_changes_bytes))
        | Some(Operation::RemoveMember(branch_changes_bytes))
        | Some(Operation::KeyUpdate(branch_changes_bytes))
        | Some(Operation::LeaveGroup(branch_changes_bytes)) => {
            let (opcode, public_inputs, post_verification_data) =
                get_opcode_and_input_for_art_update(
                    state,
                    id,
                    branch_changes_bytes,
                    frame_tbs.epoch,
                    is_current_epoch,
                )
                .await?;

            (opcode, public_inputs, Some(post_verification_data))
        }
        Some(Operation::Aggregated(change_bytes)) => {
            let (opcode, public_inputs, post_verification_data) =
                get_opcode_and_input_for_aggregation(state, id, change_bytes, frame_tbs.epoch - 1)
                    .await?;

            (opcode, public_inputs, Some(post_verification_data))
        }
        Some(Operation::DropGroup(_)) => {
            let (opcode, public_inputs) = get_opcode_and_input_for_drop_group(state, id).await?;

            (opcode, public_inputs, None)
        }
        None => {
            let (opcode, public_inputs, post_verification_data) =
                get_opcode_and_input_for_send_message(state, id, frame_tbs.epoch - 1).await?;

            (opcode, public_inputs, Some(post_verification_data))
        }
    };

    let verification_req = VerificationRequest {
        opcode,
        data: VerifierData::new(proof, public_inputs, associated_data),
    };

    Ok((verification_req, post_verification_data))
}

pub async fn verify_frame_applicability_by_epoch(
    state: &Container,
    id: Uuid,
    operation: Option<&Operation>,
    proposed_epoch: u64,
    current_epoch: u64,
) -> Result<(), VerificationError> {
    let applicable_epochs = match &operation {
        Some(Operation::AddMember(_)) | Some(Operation::Aggregated(_)) => {
            vec![current_epoch + 1]
        }
        Some(Operation::RemoveMember(_))
        | Some(Operation::KeyUpdate(_))
        | Some(Operation::LeaveGroup(_)) => {
            let mut applicable_epochs = vec![current_epoch + 1];

            if state.merge_changes {
                applicable_epochs.push(current_epoch);
            }

            applicable_epochs
        },
        Some(Operation::Init(_)) => vec![0],
        Some(Operation::DropGroup(_)) => vec![current_epoch + 1],
        None => vec![current_epoch],
    };

    if !applicable_epochs.contains(&proposed_epoch) {
        error!(
            group_id = ?id,
            "Invalid epoch provided ({}), while the current one is {}",
            proposed_epoch, current_epoch
        );

        return Err(VerificationError::InvalidEpoch {
            current: current_epoch,
            provided: proposed_epoch,
        });
    }

    Ok(())
}

pub async fn verify_and_send(
    verification_req: VerificationRequest,
    state: &Container,
) -> Result<(), VerificationError> {
    let verification_message = verification_req.to_message().inspect_err(|err| {
        error!(
            "Failed to convert VerificationRequest to ProofVerifierMessage: {}",
            err
        )
    })?;

    verify(verification_message, &state.proof_verifier_sender).await?;

    info!("Verification successful.");

    Ok(())
}

pub fn get_opcode_and_input_for_init_group(
    tbs_frame: &FrameTbs,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let public_key = CortadoAffine::deserialize_compressed(&*tbs_frame.nonce)?;

    Ok((
        VerificationOpcode::InitGroup,
        PublicInputs::Signature {
            public_keys: vec![public_key],
        },
    ))
}

pub async fn get_opcode_and_input_for_aggregation(
    state: &Container,
    id: Uuid,
    aggregation_bytes: &Vec<u8>,
    current_epoch: u64,
) -> Result<(VerificationOpcode, PublicInputs, PostVerificationData), VerificationError> {
    let aggregated_change: AggregatedChange<CortadoAffine> =
        postcard::from_bytes(aggregation_bytes)?;

    let mut art = state
        .art_service
        .get_latest_art(id)
        .await?
        .ok_or(VerificationError::NotFound)?
        .art;
    let post_verification_data = PostVerificationData::new(
        current_epoch + 1,
        art.root().data().public_key(),
        art.preview().root().public_key(),
    );
    art.commit()?;

    let frame_storage = MongoFramesStorage::new(&id).await?;
    let epoch_changes = frame_storage
        .get_epoch_changes(id, current_epoch + 1)
        .await?;

    if !epoch_changes.is_empty() {
        return Err(VerificationError::ExclusiveOperationAlreadyExists(
            current_epoch,
        ));
    }

    let eligibility_requirement =
        EligibilityRequirement::Previleged((get_left_most_leaf_public_key(&art).await?, vec![]));

    Ok((
        VerificationOpcode::Aggregation,
        PublicInputs::ArtAggregationInput {
            change: aggregated_change,
            art,
            eligibility_requirement,
        },
        post_verification_data,
    ))
}

pub async fn get_opcode_and_input_for_art_update(
    state: &Container,
    id: Uuid,
    branch_changes_bytes: &Vec<u8>,
    frame_epoch: u64,
    is_current_epoch: bool,
) -> Result<(VerificationOpcode, PublicInputs, PostVerificationData), VerificationError> {
    let branch_changes: BranchChange<CortadoAffine> = postcard::from_bytes(&branch_changes_bytes)?;

    let mut art = state
        .art_service
        .get_latest_art(id)
        .await?
        .ok_or(VerificationError::NotFound)?
        .art;
    let post_verification_data = PostVerificationData::new(
        frame_epoch,
        art.root().data().public_key(),
        art.preview().root().public_key(),
    );
    if !is_current_epoch {
        art.commit()?;
    }

    // Verify change applicability in correspondence to other epoch changes.
    if matches!(branch_changes.change_type, BranchChangeType::Leave)
        || matches!(branch_changes.change_type, BranchChangeType::RemoveMember)
        || matches!(branch_changes.change_type, BranchChangeType::UpdateKey)
    {
        let frame_storage = MongoFramesStorage::new(&id).await?;
        let epoch_changes = frame_storage
            .get_epoch_changes(id, frame_epoch)
            .await
            .inspect_err(|err| {
                error!(
                    "Failed to get changes for epoch {}: {}",
                    frame_epoch,
                    err
                )
            })?;

        let ArtUpdate::BranchChange(epoch_changes) = epoch_changes else {
            error!(
                "Epoch {} already contain aggregated operation, so no other changes can be applied.",
                frame_epoch,
            );
            return Err(VerificationError::ExclusiveOperationAlreadyExists(
                frame_epoch,
            ));
        };

        for change in epoch_changes {
            if change.node_index.as_index()? == branch_changes.node_index.as_index()? {
                if matches!(change.change_type, BranchChangeType::Leave)
                    || matches!(change.change_type, BranchChangeType::RemoveMember)
                {
                    if branch_changes.change_type == BranchChangeType::UpdateKey {
                        error!("Can't update key, as the user will be removed after merge.");
                        return Err(VerificationError::UserAlreadyRemoved);
                    } else {
                        error!("Can't remove the user for a second time.");
                        return Err(VerificationError::MergeUserRemove);
                    }
                }
            }

            if matches!(change.change_type, BranchChangeType::AddMember) {
                return Err(VerificationError::AddMemberUniqueness {
                    epoch: frame_epoch,
                });
            }
        }
    }

    let (opcode, eligibility_requirement) = match branch_changes.change_type {
        BranchChangeType::UpdateKey => {
            let leaf = art.node(&branch_changes.node_index)?;
            if !leaf.is_leaf() {
                error!("Fail to update art, as the node isn't leaf");
                return Err(VerificationError::InvalidInput);
            }

            if !matches!(leaf.data().status(), Some(LeafStatus::Active)) {
                error!(
                    "Fail to perform key update as the target leaf status is: {:?}.",
                    leaf.data().status()
                );
                return Err(VerificationError::UserAlreadyRemoved);
            }

            (
                VerificationOpcode::KeyUpdate,
                EligibilityRequirement::Member(
                    art.node(&branch_changes.node_index)?.data().public_key(),
                ),
            )
        }
        BranchChangeType::Leave => {
            let leaf = art.node(&branch_changes.node_index)?;
            if !matches!(leaf.data().status(), Some(LeafStatus::Active)) {
                return Err(VerificationError::UserAlreadyRemoved);
            }

            (
                VerificationOpcode::LeaveGroup,
                EligibilityRequirement::Member(
                    art.node(&branch_changes.node_index)?.data().public_key(),
                ),
            )
        }
        BranchChangeType::AddMember => (
            VerificationOpcode::AddMember,
            EligibilityRequirement::Previleged((
                get_left_most_leaf_public_key(&art).await?,
                vec![],
            )),
        ),
        BranchChangeType::RemoveMember => {
            let target_leaf = art.node(&branch_changes.node_index)?;

            if !target_leaf.is_leaf() {
                error!("Target leaf for removal is not a leaf.");
                return Err(VerificationError::InvalidInput);
            }

            let eligibility = if matches!(target_leaf.data().status(), Some(LeafStatus::Active)) {
                EligibilityRequirement::Previleged((
                    get_left_most_leaf_public_key(&art).await?,
                    vec![],
                ))
            } else {
                EligibilityRequirement::Member(art.root().data().public_key())
            };

            debug!(
                "Using the next eligibility for remove member verification: {:?}",
                eligibility
            );

            (VerificationOpcode::RemoveMember, eligibility)
        }
    };

    Ok((
        opcode,
        PublicInputs::ArtUpdateInput {
            change: branch_changes,
            art,
            eligibility_requirement,
        },
        post_verification_data,
    ))
}

pub async fn get_left_most_leaf_public_key(
    art: &PublicArt<CortadoAffine>,
) -> Result<CortadoAffine, VerificationError> {
    let mut left_most_leaf = art.root();
    while let Some(node) = left_most_leaf.child(Direction::Left) {
        left_most_leaf = node;
    }

    Ok(left_most_leaf.data().public_key())
}

pub async fn get_opcode_and_input_for_drop_group(
    state: &Container,
    id: Uuid,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let art = state.art_service.get_art(id, None).await?.art;

    let mut left_most_leaf = art.root();
    let mut path = Vec::new();
    while let Some(node) = left_most_leaf.child(Direction::Left) {
        path.push(Direction::Left);
        left_most_leaf = node;
    }

    let leaf = art.node(&NodeIndex::Direction(path))?;
    if !leaf.is_leaf() {
        return Err(VerificationError::InvalidInput);
    }

    Ok((
        VerificationOpcode::DeleteChat,
        PublicInputs::Signature {
            public_keys: vec![leaf.data().public_key()],
        },
    ))
}

pub async fn get_opcode_and_input_for_send_message(
    state: &Container,
    id: Uuid,
    current_epoch: u64,
) -> Result<(VerificationOpcode, PublicInputs, PostVerificationData), VerificationError> {
    let art = state.art_service.get_art(id, None).await?.art;
    let post_verification_data = PostVerificationData::new(
        current_epoch + 1,
        art.root().data().public_key(),
        art.preview().root().public_key(),
    );

    Ok((
        VerificationOpcode::SendMessage,
        PublicInputs::Signature {
            public_keys: vec![art.preview().root().public_key()],
        },
        post_verification_data,
    ))
}

pub async fn get_input_for_send_message_with_lock(
    state: &Container,
    id: Uuid,
    session: &mut ClientSession,
) -> Result<CortadoAffine, VerificationError> {
    let art = state
        .art_service
        .get_latest_art_in_session_with_lock(id, &mut *session)
        .await?
        .ok_or(VerificationError::NotFound)?
        .art;

    Ok(art.preview().root().public_key())
}

pub async fn verdict(
    message: ProofVerifierMessage,
    proof_verifier_sender: &ProofVerifierSender,
) -> Result<bool, VerificationError> {
    match message {
        ProofVerifierMessage::ArtUpdate { .. } => {
            match callback(proof_verifier_sender, message).await? {
                ProofVerifierResult::ArtUpdate { verdict } => Ok(verdict),
                _ => Err(VerificationError::InvalidResultMessage),
            }
        }
        ProofVerifierMessage::ArtAggregation { .. } => {
            match callback(proof_verifier_sender, message).await? {
                ProofVerifierResult::ArtAggregation { verdict } => Ok(verdict),
                _ => Err(VerificationError::InvalidResultMessage),
            }
        }
        ProofVerifierMessage::SchnorrSignature { .. } => {
            match callback(proof_verifier_sender, message).await? {
                ProofVerifierResult::SchnorrSignature { verdict } => Ok(verdict),
                _ => Err(VerificationError::InvalidResultMessage),
            }
        }
    }
}

pub async fn verify(
    message: ProofVerifierMessage,
    proof_verifier_sender: &ProofVerifierSender,
) -> Result<(), VerificationError> {
    let verdict = verdict(message, proof_verifier_sender).await?;

    match verdict {
        true => Ok(()),
        false => Err(VerificationError::InvalidProof),
    }
}
