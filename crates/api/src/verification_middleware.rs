use crate::{Container, MessengerService};
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
use mongodb::bson::doc;
use proof_verifier::ProofVerifierSender;
use proof_verifier::verifier_engine::*;
use prost::Message;
use sha3::{Digest, Sha3_256};
use std::sync::Arc;
use storage::{ARTStorage, MongoARTStorage};
use tracing::{debug, error};
use types::art_schemas::{GetARTQuery, ProofMode};
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use types::centrifugo_schemas::AuthRequest;
use types::errors::ServiceError::MessageServiceError;
use types::errors::{ARTServiceError, VerificationError};
use types::messenger_schemas::GetMessageQuery;
use types::protos::{Frame, FrameTbs, group_operation::Operation};
use uuid::Uuid;
use zrt_art::traits::{ARTPublicAPI, ARTPublicView};
use zrt_art::types::{BranchChanges, BranchChangesType, Direction, LeafIter, NodeIndex};

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

        root_keys.push(art.get_root().public_key);
    }

    let verification_req = VerificationRequest {
        opcode: VerificationOpcode::AuthRequest,
        data: VerifierData {
            proof: auth_request.proof,
            public_inputs: PublicInputs::Signature {
                public_keys: root_keys,
            },
            context: Sha3_256::digest(auth_request.challenge).to_vec(),
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
                public_keys: vec![art.root.public_key],
            },
            context: msg,
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
            for node in LeafIter::new(art.get_root()) {
                if node.public_key.eq(&public_key) {
                    public_key_is_wrong = false;
                }
            }

            if public_key_is_wrong {
                error!("Provided leaf public key doesn't match any leaf in the tree.");
                return Err(VerificationError::InvalidInput);
            }
        }
        ProofMode::UseRootKey => {
            if art.get_root().public_key != public_key {
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
            context: msg,
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
        error!("Group ID mismatch");
        return Err(VerificationError::InvalidInput);
    }

    let associated_data = Sha3_256::digest(tbs_frame.encode_to_vec()).to_vec();

    let operation = match &tbs_frame.group_operation {
        None => None,
        Some(val) => val.operation.as_ref(),
    };

    validate_frame_applicability(state.clone(), id, &tbs_frame).await?;

    let (opcode, public_inputs) = match &operation {
        None => get_opcode_and_input_for_send_message(state.clone(), id).await?,
        Some(Operation::Init(_)) => get_opcode_and_input_for_init_group(&tbs_frame)?,
        Some(Operation::AddMember(branch_changes_bytes))
        | Some(Operation::RemoveMember(branch_changes_bytes))
        | Some(Operation::KeyUpdate(branch_changes_bytes)) => {
            state
                .start_updating(id)
                .await
                .map_err(VerificationError::from)?;

            match get_opcode_and_input_for_art_update(
                state.clone(),
                id,
                branch_changes_bytes,
                Some(tbs_frame.epoch - 1),
            )
            .await
            {
                Ok(result) => result,
                Err(err) => {
                    state.stop_updating(id).await;
                    return Err(err);
                }
            }
        }
        Some(Operation::LeaveGroup(index)) => {
            get_opcode_and_input_for_leave_group(state.clone(), id, *index).await?
        }
        Some(Operation::DropGroup(_)) => {
            get_opcode_and_input_for_drop_group(state.clone(), id).await?
        }
    };

    let verification_req = VerificationRequest {
        opcode,
        data: VerifierData {
            proof: frame.proof,
            public_inputs,
            context: associated_data,
        },
    };

    let response = verify_and_send(verification_req, state.clone(), next, parts, bytes).await;
    match &operation {
        Some(Operation::AddMember(_))
        | Some(Operation::RemoveMember(_))
        | Some(Operation::KeyUpdate(_)) => {
            state.stop_updating(id).await;
        }
        _ => {}
    }

    response
}

async fn validate_frame_applicability(
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

    // if !state.merge_changes {
    let applicable_epochs = match &operation {
        Some(Operation::AddMember(_)) => {
            vec![current_epoch + 1]
        }
        Some(Operation::RemoveMember(_)) | Some(Operation::KeyUpdate(_)) => {
            match state.merge_changes {
                true => vec![current_epoch, current_epoch + 1],
                false => vec![current_epoch + 1],
            }
        }
        _ => {
            // Allow all epochs. For validation allow the used one.
            vec![tbs_frame.epoch]
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

    // If merge_changes feature is disabled, allow only frames, with epoch following the current one.
    match &operation {
        Some(Operation::RemoveMember(_)) | Some(Operation::KeyUpdate(_)) => {
            if state
                .messenger_service
                .epoch_has_add_member_change(id, tbs_frame.epoch)
                .await?
            {
                return Err(VerificationError::AddMemberUniqueness {
                    epoch: tbs_frame.epoch,
                });
            }
        }
        Some(Operation::AddMember(_)) => {
            if state
                .messenger_service
                .count_messages(id, doc! {"epoch": tbs_frame.epoch as i64 }, 1, 0)
                .await?
                != 0
            {
                return Err(VerificationError::AddMemberUniqueness {
                    epoch: tbs_frame.epoch,
                });
            }
        }
        _ => {}
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
    verify(verification_req.to_message()?, &state.proof_verifier_sender).await?;

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

pub async fn get_opcode_and_input_for_art_update(
    state: Arc<Container>,
    id: Uuid,
    branch_changes_bytes: &Vec<u8>,
    epoch: Option<u64>,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let branch_changes =
        BranchChanges::<CortadoAffine>::deserialize(branch_changes_bytes.as_slice())?;

    let art = state.art_service.get_art(id, epoch).await?.art;
    let verification_artefacts = art.compute_artefacts_for_verification(&branch_changes)?;

    let (opcode, aux_public_keys) = match branch_changes.change_type {
        BranchChangesType::UpdateKey => (
            VerificationOpcode::KeyUpdate,
            vec![art.get_node(&branch_changes.node_index)?.public_key],
        ),
        BranchChangesType::AppendNode => (
            VerificationOpcode::AddMember,
            vec![get_left_most_leaf_public_key(state, id).await?],
        ),
        BranchChangesType::MakeBlank => {
            let aux_public_key = match art.get_node(&branch_changes.node_index)?.is_blank {
                true => art.root.public_key,
                false => get_left_most_leaf_public_key(state, id).await?,
            };

            (VerificationOpcode::RemoveMember, vec![aux_public_key])
        }
    };

    Ok((
        opcode,
        PublicInputs::ArtUpdateInput {
            aux_public_keys,
            path: verification_artefacts.path,
            co_path: verification_artefacts.co_path,
        },
    ))
}

pub async fn get_left_most_leaf_public_key(
    state: Arc<Container>,
    id: Uuid,
) -> Result<CortadoAffine, VerificationError> {
    let art = state.art_service.get_art(id, None).await?.art;

    let mut left_most_leaf = art.root.as_ref();
    while let Ok(node) = left_most_leaf.get_left() {
        left_most_leaf = node;
    }

    Ok(left_most_leaf.public_key)
}

pub async fn get_opcode_and_input_for_drop_group(
    state: Arc<Container>,
    id: Uuid,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let art = state.art_service.get_art(id, None).await?.art;

    let mut left_most_leaf = art.root.as_ref();
    let mut path = Vec::new();
    while let Ok(node) = left_most_leaf.get_left() {
        path.push(Direction::Left);
        left_most_leaf = node;
    }

    let leaf = art.get_node(&NodeIndex::Direction(path))?;
    if !leaf.is_leaf() {
        return Err(VerificationError::InvalidProof);
    }

    Ok((
        VerificationOpcode::DeleteChat,
        PublicInputs::Signature {
            public_keys: vec![leaf.public_key],
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
            public_keys: vec![art.root.public_key],
        },
    ))
}

pub async fn get_opcode_and_input_for_leave_group(
    state: Arc<Container>,
    id: Uuid,
    user_index: u64,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let art = state.art_service.get_art(id, None).await?.art;

    let leaf = art.get_node(&NodeIndex::from(user_index))?;

    if !leaf.is_leaf() {
        error!("Provided index {} points on non leaf node", user_index);
        return Err(VerificationError::InvalidInput);
    }

    Ok((
        VerificationOpcode::LeaveGroup,
        PublicInputs::Signature {
            public_keys: vec![leaf.public_key],
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
        ProofVerifierMessage::SchnorrSignature { .. } => {
            match callback(proof_verifier_sender, message).await? {
                ProofVerifierResult::SchnorrSignature { verdict } => verdict,
                _ => return Err(VerificationError::InvalidResultMessage),
            }
        }
    };

    match verdict {
        true => {
            debug!("Verification successful");
            Ok(())
        }
        false => Err(VerificationError::InvalidProof),
    }
}
