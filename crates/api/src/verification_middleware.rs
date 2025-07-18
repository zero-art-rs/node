use crate::Container;
use crate::as_base64;
use crate::domains::art::transport::http::{
    AddMemberRequest, DeleteChatQuery, GetARTQuery, GetChangesQuery, GetInitialARTQuery,
    InitChatRequest, RemoveMemberRequest, UpdateKeyRequest,
};
use crate::domains::art::transport::utils::decode_branch_changes;
use crate::domains::messenger::transport::http::{
    DeleteMessageQuery, GetMessageQuery, SendMessageRequest,
};
use crate::errors::ApiError;
use art::traits::ARTPublicAPI;
use art::types::{BranchChanges, BranchChangesType, NodeIndex};
use axum::extract::State;
use axum::middleware::Next;
use axum_core::body::Body;
use axum_core::extract::Request;
use axum_core::response::{IntoResponse, Response};
use callbacks::callback;
use cortado::CortadoAffine as ARTGroup;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtUpdateRequest {
    /// Serialised BranchChanges structure
    #[serde(with = "as_base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub proof: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

pub async fn verification_middleware(
    State(state): State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let new_request = Request::from_parts(parts, Body::from(bytes.clone()));
    let b = bytes.as_ref();

    // Message proofs
    if let Ok(request) = serde_json::from_slice::<SendMessageRequest>(b) {
        return match verify_previous_root_knowledge(
            state.clone(),
            &request.chat_id,
            request.sequence_number,
            request.nonce,
            request.signature,
        )
        .await
        {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(query) = serde_json::from_slice::<GetMessageQuery>(b) {
        return match verify_previous_root_knowledge(
            state.clone(),
            &query.chat_id,
            query.sequence_number,
            query.nonce,
            query.signature,
        )
        .await
        {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(query) = serde_json::from_slice::<DeleteMessageQuery>(b) {
        return match verify_previous_root_knowledge(
            state.clone(),
            &query.chat_id,
            query.sequence_number,
            query.nonce,
            query.signature,
        )
        .await
        {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    // ART proofs
    if let Ok(_) = serde_json::from_slice::<InitChatRequest>(b) {
        return next.run(new_request).await;
    }

    if let Ok(update_request) = serde_json::from_slice::<ArtUpdateRequest>(b) {
        let branch_changes = match decode_branch_changes(&update_request.branch_changes) {
            Ok(branch_changes) => branch_changes,
            Err(e) => return e.into_response(),
        };

        let verification_result = match branch_changes.change_type {
            BranchChangesType::UpdateKey => {
                verify_update_key(
                    state.clone(),
                    &update_request.branch_changes,
                    &update_request.chat_id,
                    update_request.proof,
                )
                .await
            }
            BranchChangesType::AppendNode(_) => {
                verify_add_member(
                    state.clone(),
                    &update_request.branch_changes,
                    &update_request.chat_id,
                    update_request.proof,
                )
                .await
            }
            BranchChangesType::MakeBlank(_, _) => {
                verify_remove_member(
                    state.clone(),
                    &update_request.branch_changes,
                    &update_request.chat_id,
                    update_request.proof,
                )
                .await
            }
            _ => {
                return ApiError::BadRequest("ART operation isn't supported".to_string())
                    .into_response();
            }
        };

        return match verification_result {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(query) = serde_json::from_slice::<GetARTQuery>(b) {
        return match verify_previous_root_knowledge(
            state.clone(),
            &query.chat_id,
            query.sequence_number,
            query.nonce,
            query.signature,
        )
        .await
        {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(query) = serde_json::from_slice::<GetChangesQuery>(b) {
        return match verify_previous_root_knowledge(
            state.clone(),
            &query.chat_id,
            query.sequence_number,
            query.nonce,
            query.signature,
        )
        .await
        {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(query) = serde_json::from_slice::<GetInitialARTQuery>(b) {
        return match verify_get_initial_art_query(
            state.clone(),
            &query.chat_id,
            query.nonce,
            query.index,
            query.signature,
        )
        .await
        {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(query) = serde_json::from_slice::<DeleteChatQuery>(b) {
        return match verify_ownership(
            state.clone(),
            &query.chat_id,
            query.sequence_number,
            query.nonce,
            query.signature,
        )
        .await
        {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    // ApiError::BadRequest("Unknown request".to_string()).into_response()

    error!("Unknown request occurred");
    next.run(new_request).await
}

async fn verify_previous_root_knowledge(
    state: Arc<Container>,
    chat_id: &Uuid,
    sequence_number: Option<i64>,
    nonce: Vec<u8>,
    signature: Vec<u8>,
) -> Result<(), ApiError> {
    let previous_art_record = state
        .art_service
        .get_previous_art(chat_id, sequence_number)
        .await?;

    let mut msg = Vec::new();
    msg.extend_from_slice(chat_id.as_bytes());
    msg.extend(nonce);

    let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
        signature,
        public_keys: vec![previous_art_record.art.root.public_key],
        msg,
    };

    match callback(&state.proof_verifier_sender, schnorr_signature_message).await {
        Ok(message) => {
            let ProofVerifierResult::SchnorrSignature { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    Ok(())
}

async fn verify_ownership(
    state: Arc<Container>,
    chat_id: &Uuid,
    sequence_number: Option<i64>,
    nonce: Vec<u8>,
    signature: Vec<u8>,
) -> Result<(), ApiError> {
    let previous_art_record = state
        .art_service
        .get_previous_art(chat_id, sequence_number)
        .await?;

    let mut msg = Vec::new();
    msg.extend_from_slice(chat_id.as_bytes());
    msg.extend(nonce);

    let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
        signature,
        public_keys: vec![previous_art_record.art.root.public_key],
        msg,
    };

    match callback(&state.proof_verifier_sender, schnorr_signature_message).await {
        Ok(message) => {
            let ProofVerifierResult::SchnorrSignature { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    Ok(())
}

async fn verify_get_initial_art_query(
    state: Arc<Container>,
    chat_id: &Uuid,
    nonce: Vec<u8>,
    index: u32,
    signature: Vec<u8>,
) -> Result<(), ApiError> {
    let mut initial_art_record = state
        .art_service
        .get_initial_art(chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let leaf_node = initial_art_record.art.get_node(NodeIndex::Index(index))?;
    if !leaf_node.is_leaf() {
        return Err(ApiError::BadRequest("The node isn't a leaf".to_string()));
    }

    let mut msg = Vec::new();
    msg.extend_from_slice(chat_id.as_bytes());
    msg.extend(nonce);
    msg.extend(index.to_le_bytes());

    let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
        signature,
        public_keys: vec![leaf_node.public_key],
        msg,
    };

    match callback(&state.proof_verifier_sender, schnorr_signature_message).await {
        Ok(message) => {
            let ProofVerifierResult::SchnorrSignature { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    Ok(())
}

async fn verify_update_key(
    state: Arc<Container>,
    branch_changes: &Vec<u8>,
    chat_id: &Uuid,
    proof: Vec<u8>,
) -> Result<(), ApiError> {
    let branch_changes = decode_branch_changes(branch_changes)?;

    let art = state.art_service.get_art(chat_id, None).await?.art;

    let associated_data = art.serialize()?;
    let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

    let key_update_message = ProofVerifierMessage::KeyUpdate {
        proof,
        co_path,
        associated_data,
    };

    match callback(&state.proof_verifier_sender, key_update_message).await {
        Ok(message) => {
            let ProofVerifierResult::KeyUpdate { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    Ok(())
}

async fn verify_add_member(
    state: Arc<Container>,
    branch_changes: &Vec<u8>,
    chat_id: &Uuid,
    proof: Vec<u8>,
) -> Result<(), ApiError> {
    let branch_changes = decode_branch_changes(branch_changes)?;

    let art = state
        .art_service
        .get_art(chat_id, None)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?
        .art;

    let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

    let add_member_message = ProofVerifierMessage::AddMember {
        proof,
        co_path,
        associated_data: art.serialize()?,
    };

    match callback(&state.proof_verifier_sender, add_member_message).await {
        Ok(message) => {
            let ProofVerifierResult::AddMember { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    Ok(())
}

async fn verify_remove_member(
    state: Arc<Container>,
    branch_changes: &Vec<u8>,
    chat_id: &Uuid,
    proof: Vec<u8>,
) -> Result<(), ApiError> {
    let branch_changes = decode_branch_changes(branch_changes)?;

    let art = state.art_service.get_art(&chat_id, None).await?.art;

    let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

    let remove_member_message = ProofVerifierMessage::RemoveMember {
        proof,
        co_path,
        associated_data: art.serialize()?,
    };

    match callback(&state.proof_verifier_sender, remove_member_message).await {
        Ok(message) => {
            let ProofVerifierResult::RemoveMember { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    Ok(())
}
