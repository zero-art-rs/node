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
use proof_verifier::ProofVerifierSender;
use proof_verifier::verifier_engine::*;
use prost::Message;
use sha3::{Digest, Sha3_256};
use std::sync::Arc;
use storage::{ARTStorage, FrameStorage, MongoARTStorage, MongoFramesStorage};
use tracing::{debug, error, info, trace};
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

        root_keys.push(art.root().public_key());
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

    let art = state
        .art_service
        .get_art(chat_id, Some(payload.epoch.unwrap_or(0)))
        .await?
        .art;

    let mut msg = Vec::new();
    msg.extend_from_slice(chat_id.as_bytes());
    msg.extend(&payload.nonce);
    let msg = Sha3_256::digest(&msg).to_vec();

    let verification_req = VerificationRequest {
        opcode: VerificationOpcode::GetMessages,
        data: VerifierData {
            proof: payload.signature.clone(),
            public_inputs: PublicInputs::Signature {
                public_keys: vec![art.root().public_key()],
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
                if node.public_key().eq(&public_key) {
                    public_key_is_wrong = false;
                }
            }

            if public_key_is_wrong {
                error!("Provided leaf public key doesn't match any leaf in the tree.");
                return Err(VerificationError::InvalidInput);
            }
        }
        ProofMode::UseRootKey => {
            if art.root().public_key() != public_key {
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

/// Handle send_frame verification
pub async fn send_frame(
    State(state): State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Result<Response, VerificationError> {
    debug!(
        "Incoming send frame verification request: {} {}.",
        request.method(),
        request.uri(),
    );

    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await?;

    let Path(id) = Path::<Uuid>::from_request_parts(&mut parts.clone(), &state).await?;

    let frame = Frame::decode(bytes.clone())?;
    let tbs_frame = frame.frame.ok_or_else(|| ARTServiceError::InvalidInput)?;

    if tbs_frame.group_id != id.to_string() {
        error!(
            "Group ID mismatch: tbs_frame.group_id is {}, while id in path is {}",
            tbs_frame.group_id, id
        );
        return Err(VerificationError::InvalidInput);
    }

    let associated_data = Sha3_256::digest(tbs_frame.encode_to_vec()).to_vec();

    let operation = match &tbs_frame.group_operation {
        None => None,
        Some(val) => val.operation.as_ref(),
    };

    verify_frame_applicability_by_epoch(state.clone(), id, &tbs_frame).await?;

    let (opcode, public_inputs) = match &operation {
        Some(Operation::Init(_)) => get_opcode_and_input_for_init_group(&tbs_frame)?,
        Some(Operation::AddMember(branch_changes_bytes))
        | Some(Operation::RemoveMember(branch_changes_bytes))
        | Some(Operation::KeyUpdate(branch_changes_bytes))
        | Some(Operation::LeaveGroup(branch_changes_bytes)) => {
            state
                .start_updating(id)
                .await
                .map_err(VerificationError::from)?;

            match get_opcode_and_input_for_art_update(
                state.clone(),
                id,
                branch_changes_bytes,
                tbs_frame.epoch - 1,
            )
            .await
            {
                Ok(result) => result,
                Err(err) => {
                    error!("Failed to get opcode and operation: {}", err.to_string());
                    state.stop_updating(id).await;
                    return Err(err);
                }
            }
        }
        Some(Operation::Aggregated(change_bytes)) => {
            state
                .start_updating(id)
                .await
                .map_err(VerificationError::from)?;

            match get_opcode_and_input_for_aggregation(
                state.clone(),
                id,
                change_bytes,
                tbs_frame.epoch - 1,
            )
            .await
            {
                Ok(result) => result,
                Err(err) => {
                    error!("Failed to get opcode and operation");
                    state.stop_updating(id).await;
                    return Err(err);
                }
            }
        }
        Some(Operation::DropGroup(_)) => {
            get_opcode_and_input_for_drop_group(state.clone(), id).await?
        }
        None => get_opcode_and_input_for_send_message(state.clone(), id).await?,
    };

    let verification_req = VerificationRequest {
        opcode,
        data: VerifierData {
            proof: frame.proof,
            public_inputs,
            associated_data,
        },
    };

    let response = verify_and_send(verification_req, state.clone(), next, parts, bytes).await;

    match &operation {
        Some(Operation::AddMember(_))
        | Some(Operation::RemoveMember(_))
        | Some(Operation::KeyUpdate(_))
        | Some(Operation::LeaveGroup(_))
        | Some(Operation::Aggregated(_)) => {
            state.stop_updating(id).await;
        }
        _ => {}
    }

    response
}

async fn verify_frame_applicability_by_epoch(
    state: Arc<Container>,
    id: Uuid,
    tbs_frame: &FrameTbs,
) -> Result<(), VerificationError> {
    let operation = match &tbs_frame.group_operation {
        None => None,
        Some(val) => val.operation.as_ref(),
    };

    let current_epoch = MongoARTStorage::new()
        .await?
        .get_current_epoch(&id)
        .await
        .unwrap_or(0);

    let applicable_epochs = match &operation {
        Some(Operation::AddMember(_)) => {
            vec![current_epoch + 1]
        }
        Some(Operation::RemoveMember(_))
        | Some(Operation::KeyUpdate(_))
        | Some(Operation::LeaveGroup(_)) => match state.merge_changes {
            true => vec![current_epoch, current_epoch + 1],
            false => vec![current_epoch + 1],
        },
        _ => {
            // Allow all epochs. For validation allow the used one.
            vec![current_epoch, current_epoch + 1]
        }
    };

    if !applicable_epochs.contains(&tbs_frame.epoch) {
        error!(
            "Invalid epoch provided ({}), while the current one is {}",
            tbs_frame.epoch, current_epoch
        );
        return Err(VerificationError::InvalidEpoch {
            current: current_epoch,
            provided: tbs_frame.epoch,
        });
    }

    Ok(())
}

pub async fn verify_and_send(
    verification_req: VerificationRequest,
    state: Arc<Container>,
    next: Next,
    parts: Parts,
    bytes: Bytes,
) -> Result<Response, VerificationError> {
    let verification_message = verification_req
        .to_message()
        .inspect_err(|err| error!("Failed to send frame: {}", err))?;
    verify(verification_message, &state.proof_verifier_sender).await?;

    debug!("Create response");
    Ok(next
        .run(Request::from_parts(
            parts.clone(),
            Body::from(bytes.clone()),
        ))
        .await)
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
    state: Arc<Container>,
    id: Uuid,
    aggregation_bytes: &Vec<u8>,
    current_epoch: u64,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let aggregated_change: AggregatedChange<CortadoAffine> =
        postcard::from_bytes(aggregation_bytes)?;

    let art = state
        .art_service
        .get_art(id, Some(current_epoch))
        .await?
        .art;

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
    ))
}

pub async fn get_opcode_and_input_for_art_update(
    state: Arc<Container>,
    id: Uuid,
    branch_changes_bytes: &Vec<u8>,
    current_epoch: u64,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let branch_changes: BranchChange<CortadoAffine> = postcard::from_bytes(&branch_changes_bytes)?;

    let art = state
        .art_service
        .get_art(id, Some(current_epoch))
        .await?
        .art;

    // Verify change applicability in correspondence to other epoch changes.
    if matches!(branch_changes.change_type, BranchChangeType::Leave)
        || matches!(branch_changes.change_type, BranchChangeType::RemoveMember)
        || matches!(branch_changes.change_type, BranchChangeType::UpdateKey)
    {
        let frame_storage = MongoFramesStorage::new(&id).await?;
        let epoch_changes = frame_storage
            .get_epoch_changes(id, current_epoch + 1)
            .await
            .inspect_err(|err| {
                error!(
                    "Failed to get changes for epoch {}: {}",
                    current_epoch + 1,
                    err
                )
            })?;

        let ArtUpdate::BranchChange(epoch_changes) = epoch_changes else {
            error!(
                "Epoch {} already contain aggregated operation, so no other changes can be applied.",
                current_epoch + 1,
            );
            return Err(VerificationError::ExclusiveOperationAlreadyExists(
                current_epoch + 1,
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
                    epoch: current_epoch,
                });
            }
        }
    }

    let (opcode, eligibility_requirement) = match branch_changes.change_type {
        BranchChangeType::UpdateKey => {
            let leaf = art.node(&branch_changes.node_index)?;
            if !leaf.is_leaf() {
                error!(
                    "Fail to update art, as the node isn't leaf. ArtTree is\n{}",
                    art.root()
                );
                return Err(VerificationError::InvalidInput);
            }

            if !matches!(leaf.status(), Some(LeafStatus::Active)) {
                error!(
                    "Fail to perform key update as the target leaf status is: {:?}.",
                    leaf.status()
                );
                return Err(VerificationError::UserAlreadyRemoved);
            }

            (
                VerificationOpcode::KeyUpdate,
                EligibilityRequirement::Member(art.node(&branch_changes.node_index)?.public_key()),
            )
        }
        BranchChangeType::Leave => {
            let leaf = art.node(&branch_changes.node_index)?;
            if !matches!(leaf.status(), Some(LeafStatus::Active)) {
                return Err(VerificationError::UserAlreadyRemoved);
            }

            (
                VerificationOpcode::LeaveGroup,
                EligibilityRequirement::Member(art.node(&branch_changes.node_index)?.public_key()),
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

            let eligibility = if matches!(target_leaf.status(), Some(LeafStatus::Active)) {
                EligibilityRequirement::Previleged((
                    get_left_most_leaf_public_key(&art).await?,
                    vec![],
                ))
            } else {
                EligibilityRequirement::Member(art.root().public_key())
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
    ))
}

pub async fn get_left_most_leaf_public_key(
    art: &PublicArt<CortadoAffine>,
) -> Result<CortadoAffine, VerificationError> {
    let mut left_most_leaf = art.root();
    while let Some(node) = left_most_leaf.child(Direction::Left) {
        left_most_leaf = node;
    }

    Ok(left_most_leaf.public_key())
}

pub async fn get_opcode_and_input_for_drop_group(
    state: Arc<Container>,
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
        return Err(VerificationError::InvalidProof);
    }

    Ok((
        VerificationOpcode::DeleteChat,
        PublicInputs::Signature {
            public_keys: vec![leaf.public_key()],
        },
    ))
}

pub async fn get_opcode_and_input_for_send_message(
    state: Arc<Container>,
    id: Uuid,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let art = state.art_service.get_art(id, None).await?.art;

    Ok((
        VerificationOpcode::SendMessage,
        PublicInputs::Signature {
            public_keys: vec![art.root().public_key()],
        },
    ))
}

pub async fn verify(
    message: ProofVerifierMessage,
    proof_verifier_sender: &ProofVerifierSender,
) -> Result<(), VerificationError> {
    let verdict = match message {
        ProofVerifierMessage::ArtUpdate { .. } => {
            match callback(proof_verifier_sender, message).await? {
                ProofVerifierResult::ArtUpdate { verdict } => verdict,
                _ => return Err(VerificationError::InvalidResultMessage),
            }
        }
        ProofVerifierMessage::ArtAggregation { .. } => {
            match callback(proof_verifier_sender, message).await? {
                ProofVerifierResult::ArtAggregation { verdict } => verdict,
                _ => return Err(VerificationError::InvalidResultMessage),
            }
        }
        ProofVerifierMessage::SchnorrSignature { .. } => {
            match callback(proof_verifier_sender, message).await? {
                ProofVerifierResult::SchnorrSignature { verdict } => verdict,
                _ => return Err(VerificationError::InvalidResultMessage),
            }
        }
    };

    match verdict {
        true => Ok(()),
        false => Err(VerificationError::InvalidProof),
    }
}
