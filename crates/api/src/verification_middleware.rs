use crate::Container;
use ark_serialize::CanonicalDeserialize;
use art::traits::{ARTPublicAPI, ARTPublicView};
use art::types::{BranchChanges, BranchChangesType, Direction, LeafIterWithPath, NodeIndex};
use axum::Json;
use axum::extract::{Path, State};
use axum::middleware::Next;
use axum_core::body::Body;
use axum_core::extract::{FromRequestParts, Request};
use axum_core::response::{IntoResponse, Response};
use bytes::BytesMut;
use callbacks::callback;
use cortado::CortadoAffine;
use proof_verifier::ProofVerifierSender;
use proof_verifier::verifier_engine::*;
use prost::Message;
use std::sync::Arc;
use tracing::{debug, error, warn};
use types::art_schemas::{GetARTQuery, ProofMode};
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use types::centrifugo_schemas::AuthRequest;
use types::errors::ARTServiceError;
use types::messenger_schemas::GetMessageQuery;
use types::protos::group_operation::Operation;
use types::protos::{Frame, GroupOperation};
use types::{
    RouteId, add_route_id,
    errors::{ApiError, VerificationError},
};
use uuid::Uuid;

pub async fn verification_middleware(
    state: State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Response {
    #[cfg(not(feature = "verification"))]
    {
        // warn!("Verification middleware is disabled.");
        return next.run(request).await;
    }
    #[cfg(feature = "verification")]
    {
        verification_middleware_inner(state, request, next)
            .await
            .unwrap_or_else(|error| {
                error!("{}", error);
                ApiError::from(error).into_response()
            })
    }
}

async fn verification_middleware_inner(
    State(state): State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Result<Response, VerificationError> {
    let route_id = request
        .extensions()
        .get::<RouteId>()
        .map(|id| id.0)
        .ok_or(VerificationError::UnknownEndpoint)?;

    debug!(
        "Incoming verification request: {} {}, handled by route {}",
        request.method(),
        request.uri(),
        route_id,
    );

    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await?;
    let query = parts.uri.query();

    let verification_req = match route_id {
        "authenticate" => {
            let Json(payload) = Json::<AuthRequest>::from_bytes(&bytes)?;

            if payload.chat_ids.len() != payload.epochs.len() {
                return Err(VerificationError::InvalidInput);
            }

            let mut root_keys = Vec::new();
            for (chat_id, epoch) in payload.chat_ids.iter().zip(payload.epochs.iter()) {
                let art = state.art_service.get_art(&chat_id, Some(*epoch)).await?.art;

                root_keys.push(art.get_root().public_key);
            }

            Some(VerificationRequest {
                opcode: VerificationOpcode::AuthRequest,
                data: VerifierData {
                    proof: payload.proof,
                    public_inputs: PublicInputs::Signature {
                        public_keys: root_keys,
                    },
                    context: payload.challenge,
                },
            })
        }
        "list_messages" | "count_messages" => {
            let query_bytes = query.ok_or(VerificationError::MissingQuery)?.as_bytes();

            let Path(chat_id) =
                Path::<Uuid>::from_request_parts(&mut parts.clone(), &state).await?;
            let payload = serde_urlencoded::from_bytes::<GetMessageQuery>(query_bytes)?;

            let art = state
                .art_service
                .get_art(&chat_id, Some(payload.epoch.unwrap_or(0)))
                .await?
                .art;

            let mut msg = Vec::new();
            msg.extend_from_slice(chat_id.as_bytes());
            msg.extend(&payload.nonce);

            Some(VerificationRequest {
                opcode: VerificationOpcode::GetMessages,
                data: VerifierData {
                    proof: payload.signature.clone(),
                    public_inputs: PublicInputs::Signature {
                        public_keys: vec![art.root.public_key],
                    },
                    context: msg,
                },
            })
        }
        "get_art" => {
            let query_bytes = query.ok_or(VerificationError::MissingQuery)?.as_bytes();

            let Path((chat_id, epoch)) =
                Path::<(Uuid, i64)>::from_request_parts(&mut parts.clone(), &state).await?;
            let payload = serde_urlencoded::from_bytes::<GetARTQuery>(query_bytes)?;

            let art = state.art_service.get_art(&chat_id, Some(epoch)).await?.art;

            let public_key = CortadoAffine::deserialize_uncompressed(&*payload.public_key)
                .unwrap_or_else(|_| {
                    error!("Failed to deserialize public key");
                    CortadoAffine::default()
                });

            match ProofMode::try_from(payload.proof_mode.as_str())? {
                ProofMode::UseLeafKey => {
                    let mut public_key_is_wrong = true;
                    for (node, _) in LeafIterWithPath::new(art.get_root()) {
                        if node.public_key.eq(&public_key) {
                            public_key_is_wrong = false;
                        }
                    }

                    if public_key_is_wrong {
                        error!(
                            "Provided public key isn't correct, or the corresponding node is nor leaf, not root"
                        );
                        return Err(VerificationError::InvalidInput);
                    }
                }
                ProofMode::UseRootKey => {
                    if art.get_root().public_key != public_key {
                        return Err(VerificationError::InvalidInput);
                    }
                }
            }

            debug!("Check if provided public key is correct");

            // Context for verification
            let mut msg = Vec::new();
            msg.extend_from_slice(chat_id.as_bytes());
            msg.extend(&payload.nonce);
            msg.extend(payload.challenge);
            msg.extend(epoch.to_be_bytes());

            Some(VerificationRequest {
                opcode: VerificationOpcode::GetMessages,
                data: VerifierData {
                    proof: payload.signature.clone(),
                    public_inputs: PublicInputs::Signature {
                        public_keys: vec![public_key],
                    },
                    context: msg,
                },
            })
        }
        "send_frame" => {
            let Path(id) = Path::<Uuid>::from_request_parts(&mut parts.clone(), &state).await?;
            let frame = Frame::decode(bytes.clone())?;

            let tbs_frame = frame.frame.ok_or_else(|| ARTServiceError::InvalidInput)?;

            if tbs_frame.group_id != id.to_string() {
                error!("Group ID mismatch");
                return Err(VerificationError::InvalidInput);
            }

            let mut buf = BytesMut::new();
            tbs_frame.encode(&mut buf).unwrap();
            let associated_data = buf.to_vec();

            let operation = match &tbs_frame.group_operation {
                None => None,
                Some(val) => val.operation.as_ref(),
            };

            let operation_data = match &operation {
                None => Some(get_opcode_and_input_for_send_message(state.clone(), id).await?),
                Some(Operation::Init(_)) => {
                    return Ok(next
                        .run(Request::from_parts(parts.clone(), Body::from(bytes)))
                        .await);
                }
                Some(Operation::AddMember(branch_changes_bytes))
                | Some(Operation::RemoveMember(branch_changes_bytes))
                | Some(Operation::KeyUpdate(branch_changes_bytes)) => Some(
                    get_opcode_and_input_for_art_update(state.clone(), id, branch_changes_bytes)
                        .await?,
                ),
                Some(Operation::DropGroup(_)) => {
                    Some(get_opcode_and_input_for_drop_group(state.clone(), id).await?)
                }
            };

            match operation_data {
                Some((opcode, public_inputs)) => Some(VerificationRequest {
                    opcode,
                    data: VerifierData {
                        proof: frame.proof,
                        public_inputs,
                        context: associated_data,
                    },
                }),
                None => None,
            }
        }
        _ => return Err(VerificationError::UnknownEndpoint),
    };

    if let Some(verification_req) = verification_req {
        verify(verification_req.to_message()?, &state.proof_verifier_sender).await?;
    }

    debug!("verification completed successfully");

    Ok(next
        .run(Request::from_parts(
            parts.clone(),
            Body::from(bytes.clone()),
        ))
        .await)
}

pub async fn get_opcode_and_input_for_art_update(
    state: Arc<Container>,
    chat_id: Uuid,
    branch_changes_bytes: &Vec<u8>,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let branch_changes =
        BranchChanges::<CortadoAffine>::deserialize(branch_changes_bytes.as_slice())?;

    let art = state.art_service.get_art(&chat_id, None).await?.art;
    let verification_artefacts = art.compute_artefacts_for_verification(&branch_changes)?;

    debug!("Check aux keys correctness..");
    let (opcode, aux_public_keys) = match branch_changes.change_type {
        BranchChangesType::UpdateKey => (
            VerificationOpcode::KeyUpdate,
            vec![art.get_node(&branch_changes.node_index)?.public_key],
        ),
        BranchChangesType::AppendNode => (VerificationOpcode::AddMember, vec![art.root.public_key]),
        BranchChangesType::MakeBlank => (VerificationOpcode::MakeBlank, vec![art.root.public_key]),
        _ => return Err(VerificationError::UnsupportedOperation),
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

pub async fn get_opcode_and_input_for_drop_group(
    state: Arc<Container>,
    id: Uuid,
) -> Result<(VerificationOpcode, PublicInputs), VerificationError> {
    let art = state.art_service.get_art(&id, None).await?.art;

    let mut left_most_leaf = &art.root;
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
    let art = state.art_service.get_art(&id, None).await?.art;

    Ok((
        VerificationOpcode::SendMessage,
        PublicInputs::Signature {
            public_keys: vec![art.root.public_key],
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
        true => Ok(()),
        false => Err(VerificationError::InvalidProof),
    }
}
