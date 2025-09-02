use crate::Container;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::Method;
use axum::middleware::Next;
use axum_core::body::Body;
use axum_core::extract::{FromRequestParts, Request};
use axum_core::response::{IntoResponse, Response};
use hyper::StatusCode;
use std::sync::Arc;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::traits::{ARTPublicAPI, ARTPublicView};
use art::types::{BranchChanges, BranchChangesType, Direction, LeafIterWithPath, NodeIndex};
use cortado::CortadoAffine;
use tracing::{debug, error, warn};
use types::art_schemas::{
    AddMemberRequest, DeleteChatQuery, GetARTQuery, GetChangesQuery, RemoveMemberRequest,
    UpdateARTRequest, UpdateKeyRequest,
};
use types::messenger_schemas::{GetMessageQuery, SendMessageRequest};
use types::{
    RouteId, add_route_id,
    errors::{ApiError, VerificationError},
};
use uuid::Uuid;

use proof_verifier::verifier_engine::*;
use types::centrifugo_schemas::AuthRequest;

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

            let mut root_keys = Vec::new();
            for (chat_id, epoch) in payload.chat_ids.iter().zip(payload.epochs.iter()) {
                let art = state
                    .art_service
                    .get_art(&chat_id, Some(*epoch))
                    .await?
                    .art;

                root_keys.push(art.get_root().public_key);
            }

            VerificationRequest {
                opcode: VerificationOpcode::AuthRequest,
                data: VerifierData {
                    proof: payload.proof,
                    public_inputs: PublicInputs::Signature {
                        public_keys: root_keys,
                    },
                    context: payload.challenge,
                },
            }
        },
        "list_messages" => {
            debug!("list_messages");
            let query_bytes = query.ok_or(VerificationError::MissingQuery)?.as_bytes();

            let Path(chat_id) =
                Path::<Uuid>::from_request_parts(&mut parts.clone(), &state).await?;
            let payload = serde_urlencoded::from_bytes::<GetMessageQuery>(query_bytes)?;

            let art = state
                .art_service
                .get_art(&chat_id, payload.epoch.clone())
                .await?
                .art;

            let mut msg = Vec::new();
            msg.extend_from_slice(chat_id.as_bytes());
            msg.extend(&payload.nonce);

            VerificationRequest {
                opcode: VerificationOpcode::GetMessages,
                data: VerifierData {
                    proof: payload.signature.clone(),
                    public_inputs: PublicInputs::Signature {
                        public_keys: vec![art.root.public_key]
                    },
                    context: msg,
                },
            }
        }
        "send_message" => {
            let Path(chat_id) =
                Path::<Uuid>::from_request_parts(&mut parts.clone(), &state).await?;
            let Json(payload) = Json::<SendMessageRequest>::from_bytes(&bytes)?;

            let art = state
                .art_service
                .get_art(&chat_id, None)
                .await?
                .art;

            let mut msg = Vec::new();
            msg.extend_from_slice(chat_id.as_bytes());
            msg.extend(&payload.nonce);

            VerificationRequest {
                opcode: VerificationOpcode::SendMessage,
                data: VerifierData {
                    proof: payload.signature.clone(),
                    public_inputs: PublicInputs::Signature {
                        public_keys: vec![art.root.public_key]
                    },
                    context: msg,
                },
            }
        }
        "get_art" => {
            let query_bytes = query.ok_or(VerificationError::MissingQuery)?.as_bytes();

            let Path((chat_id, epoch)) =
                Path::<(Uuid, i64)>::from_request_parts(&mut parts.clone(), &state).await?;
            let payload = serde_urlencoded::from_bytes::<GetARTQuery>(query_bytes)?;

            let art = state
                .art_service
                .get_art(&chat_id, Some(epoch))
                .await?
                .art;

            let public_key = CortadoAffine::deserialize_uncompressed(&*payload.public_key)
                .unwrap_or_else(|_| {
                    error!("Failed to deserialize public key");
                    CortadoAffine::default()
                });

            debug!("Check if provided public key is correct");
            let mut public_key_is_wrong = true;
            for (node, _) in LeafIterWithPath::new(art.get_root()) {
                if node.public_key.eq(&public_key) {
                    public_key_is_wrong = false;
                }
            }

            if public_key_is_wrong {
                error!("Provided public key isn't correct, or the corresponding node isn't leaf");
                return Err(VerificationError::InvalidProof);
            }

            let mut msg = Vec::new();
            msg.extend_from_slice(chat_id.as_bytes());
            msg.extend(&payload.nonce);
            msg.extend(payload.challenge);

            VerificationRequest {
                opcode: VerificationOpcode::GetMessages,
                data: VerifierData {
                    proof: payload.signature.clone(),
                    public_inputs: PublicInputs::Signature {
                        public_keys: vec![public_key]
                    },
                    context: msg,
                },
            }
        }
        "get_changes" => {
            let query_bytes = query.ok_or(VerificationError::MissingQuery)?.as_bytes();

            let Path(chat_id) =
                Path::<Uuid>::from_request_parts(&mut parts.clone(), &state).await?;
            let payload = serde_urlencoded::from_bytes::<GetChangesQuery>(query_bytes)?;

            let art = state
                .art_service
                .get_art(&chat_id, Some(payload.skip))
                .await?
                .art;

            let mut msg = Vec::new();
            msg.extend_from_slice(chat_id.as_bytes());
            msg.extend(payload.nonce);

            VerificationRequest {
                opcode: VerificationOpcode::GetChanges,
                data: VerifierData {
                    proof: payload.signature,
                    public_inputs: PublicInputs::Signature {
                        public_keys: vec![art.root.public_key]
                    },
                    context: msg,
                },
            }
        }
        "update_art" => {
            warn!("update art");
            let Path(chat_id) =
                Path::<Uuid>::from_request_parts(&mut parts.clone(), &state).await?;
            let Json(payload) = Json::<UpdateARTRequest>::from_bytes(&bytes)?;

            let branch_changes = BranchChanges::<CortadoAffine>::deserialize(payload.branch_changes.as_slice())?;
            let art = state.art_service.get_art(&chat_id, None).await?.art;
            let verification_artefacts = art.compute_artefacts_for_verification(&branch_changes)?;
            let mut associated_data = Vec::new();
            art.root.public_key.serialize_uncompressed(&mut associated_data)?;

            debug!("Check aux keys correctness..");
            let (opcode, aux_public_keys) = match branch_changes.change_type {
                BranchChangesType::UpdateKey => {
                    (VerificationOpcode::KeyUpdate, vec![art.get_node(&branch_changes.node_index)?.public_key])
                }
                BranchChangesType::AppendNode => (VerificationOpcode::AddMember, vec![art.root.public_key]),
                BranchChangesType::MakeBlank => (VerificationOpcode::MakeBlank, vec![art.root.public_key]),
                _ => return Err(VerificationError::UnsupportedOperation),
            };

            VerificationRequest{
                opcode,
                data: VerifierData {
                    proof: payload.proof,
                    public_inputs: PublicInputs::ArtUpdateInput {
                        aux_public_keys,
                        path: verification_artefacts.path,
                        co_path: verification_artefacts.co_path,
                    },
                    context: associated_data,
                }
            }
        }
        "delete_chat" => {
            let query_bytes = query.ok_or(VerificationError::MissingQuery)?.as_bytes();

            let Path(chat_id) =
                Path::<Uuid>::from_request_parts(&mut parts.clone(), &state).await?;
            let payload = serde_urlencoded::from_bytes::<DeleteChatQuery>(query_bytes)?;

            let art = state.art_service.get_art(&chat_id, None).await?.art;

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

            let mut msg = Vec::new();
            msg.extend_from_slice(chat_id.as_bytes());
            msg.extend(payload.nonce);

            VerificationRequest {
                opcode: VerificationOpcode::DeleteGroup,
                data: VerifierData {
                    proof: payload.signature,
                    public_inputs: PublicInputs::Signature {
                        public_keys: vec![leaf.public_key],
                    },
                    context: msg,
                },
            }
        }
        _ => return Err(VerificationError::UnknownEndpoint),
    };

    verification_req.verify(&state.proof_verifier_sender).await?;

    debug!("verification completed successfully");
    Ok(next
        .run(Request::from_parts(
            parts.clone(),
            Body::from(bytes.clone()),
        ))
        .await)
}

