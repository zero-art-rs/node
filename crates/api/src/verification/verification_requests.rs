use crate::as_base64;
use crate::domains::art::transport::http::{
    DeleteChatQuery, GetARTQuery, GetChangesQuery, GetInitialARTQuery,
};
use crate::domains::art::transport::utils::decode_branch_changes;
use crate::domains::messenger::transport::http::{
    DeleteMessageQuery, GetMessageQuery, SendMessageRequest,
};
use crate::{Container, errors::ApiError};
use art::traits::ARTPublicAPI;
use art::types::{BranchChangesType, NodeIndex};
use axum_core::response::IntoResponse;
use callbacks::callback;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::error;
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArtUpdateRequestHelper {
    /// Serialized BranchChanges structure
    #[serde(with = "as_base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub proof: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

impl TryFrom<&[u8]> for ArtUpdateRequestHelper {
    type Error = ApiError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if let Ok(update_request) = serde_json::from_slice::<ArtUpdateRequestHelper>(value) {
            return Ok(update_request);
        }

        Err(ApiError::BadRequest(
            "Failed to decode art-update request".to_string(),
        ))
    }
}

impl ArtUpdateRequestHelper {
    pub async fn verify(&self, state: Arc<Container>) -> Result<(), ApiError> {
        let branch_changes = match decode_branch_changes(&self.branch_changes) {
            Ok(branch_changes) => branch_changes,
            Err(err) => return Err(ApiError::BadRequest(err.to_string())),
        };

        let verification_result = match branch_changes.change_type {
            BranchChangesType::UpdateKey => self.verify_update_key(state.clone()).await,
            BranchChangesType::AppendNode(_) => self.verify_add_member(state.clone()).await,
            BranchChangesType::MakeBlank(_, _) => self.verify_make_blank(state.clone()).await,
            _ => {
                return Err(ApiError::BadRequest(
                    "ART operation isn't supported".to_string(),
                ));
            }
        };

        verification_result.map_err(|err| ApiError::BadRequest(err.to_string()))
    }

    async fn verify_update_key(&self, state: Arc<Container>) -> Result<(), ApiError> {
        let branch_changes = decode_branch_changes(&self.branch_changes)?;

        let art = state.art_service.get_art(&self.chat_id, None).await?.art;

        let associated_data = art.serialize()?;
        let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

        let key_update_message = ProofVerifierMessage::KeyUpdate {
            proof: self.proof.clone(),
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

    async fn verify_add_member(&self, state: Arc<Container>) -> Result<(), ApiError> {
        let branch_changes = decode_branch_changes(&self.branch_changes)?;

        let art = state
            .art_service
            .get_art(&self.chat_id, None)
            .await
            .map_err(|e| ApiError::InternalServerError(e.to_string()))?
            .art;

        let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

        let add_member_message = ProofVerifierMessage::AddMember {
            proof: self.proof.clone(),
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

    async fn verify_make_blank(&self, state: Arc<Container>) -> Result<(), ApiError> {
        let branch_changes = decode_branch_changes(&self.branch_changes)?;

        let art = state.art_service.get_art(&self.chat_id, None).await?.art;

        let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

        let remove_member_message = ProofVerifierMessage::RemoveMember {
            proof: self.proof.clone(),
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
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RootKnowledgeProofHelper {
    chat_id: Uuid,
    sequence_number: Option<i64>,
    #[serde(with = "as_base64")]
    nonce: Vec<u8>,
    #[serde(with = "as_base64")]
    signature: Vec<u8>,
}

impl From<SendMessageRequest> for RootKnowledgeProofHelper {
    fn from(query: SendMessageRequest) -> Self {
        Self {
            chat_id: query.chat_id,
            sequence_number: None,
            nonce: query.nonce,
            signature: query.signature,
        }
    }
}

impl From<GetMessageQuery> for RootKnowledgeProofHelper {
    fn from(query: GetMessageQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            sequence_number: query.sequence_number,
            nonce: query.nonce,
            signature: query.signature,
        }
    }
}

impl From<DeleteMessageQuery> for RootKnowledgeProofHelper {
    fn from(query: DeleteMessageQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            sequence_number: query.sequence_number,
            nonce: query.nonce,
            signature: query.signature,
        }
    }
}

impl TryFrom<&[u8]> for RootKnowledgeProofHelper {
    type Error = ApiError;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if let Ok(query) = serde_json::from_slice::<SendMessageRequest>(value) {
            return Ok(Self::from(query));
        }

        if let Ok(query) = serde_json::from_slice::<GetMessageQuery>(value) {
            return Ok(Self::from(query));
        }

        if let Ok(query) = serde_json::from_slice::<DeleteMessageQuery>(value) {
            return Ok(Self::from(query));
        }

        Err(ApiError::BadRequest(
            "Failed to decode art-update request".to_string(),
        ))
    }
}

impl RootKnowledgeProofHelper {
    pub async fn verify(&self, state: Arc<Container>) -> Result<(), ApiError> {
        Ok(())
    }
    pub async fn _verify(&self, state: Arc<Container>) -> Result<(), ApiError> {
        let previous_art_record = state
            .art_service
            .get_previous_art(&self.chat_id, self.sequence_number)
            .await?;

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(&self.nonce);

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: self.signature.clone(),
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
}


#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PreviousRootKnowledgeProofHelper {
    chat_id: Uuid,
    sequence_number: Option<i64>,
    #[serde(with = "as_base64")]
    nonce: Vec<u8>,
    #[serde(with = "as_base64")]
    signature: Vec<u8>,
}

impl From<GetChangesQuery> for PreviousRootKnowledgeProofHelper {
    fn from(query: GetChangesQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            sequence_number: query.sequence_number,
            nonce: query.nonce,
            signature: query.signature,
        }
    }
}

impl From<GetARTQuery> for PreviousRootKnowledgeProofHelper {
    fn from(query: GetARTQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            sequence_number: query.sequence_number,
            nonce: query.nonce,
            signature: query.signature,
        }
    }
}

impl TryFrom<&[u8]> for PreviousRootKnowledgeProofHelper {
    type Error = ApiError;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if let Ok(query) = serde_json::from_slice::<GetChangesQuery>(value) {
            return Ok(Self::from(query));
        }

        if let Ok(query) = serde_json::from_slice::<GetARTQuery>(value) {
            return Ok(Self::from(query));
        }

        Err(ApiError::BadRequest(
            "Failed to decode art-update request".to_string(),
        ))
    }
}

impl PreviousRootKnowledgeProofHelper {
    pub async fn verify(&self, state: Arc<Container>) -> Result<(), ApiError> {
        let art_record = state
            .art_service
            .get_art(&self.chat_id, self.sequence_number)
            .await?;

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(&self.nonce);

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: self.signature.clone(),
            public_keys: vec![art_record.art.root.public_key],
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
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetInitialARTQueryHelper {
    chat_id: Uuid,
    #[serde(with = "as_base64")]
    nonce: Vec<u8>,
    index: u32,
    #[serde(with = "as_base64")]
    signature: Vec<u8>,
}

impl From<GetInitialARTQuery> for GetInitialARTQueryHelper {
    fn from(query: GetInitialARTQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            nonce: query.nonce,
            index: query.index,
            signature: query.signature,
        }
    }
}

impl TryFrom<&[u8]> for GetInitialARTQueryHelper {
    type Error = ApiError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match serde_json::from_slice::<GetInitialARTQuery>(value) {
            Ok(query) => Ok(Self::from(query)),
            Err(err) => Err(ApiError::BadRequest(err.to_string())),
        }
    }
}

impl GetInitialARTQueryHelper {
    pub async fn verify(self, state: Arc<Container>) -> Result<(), ApiError> {
        let mut initial_art_record = state
            .art_service
            .get_initial_art(&self.chat_id)
            .await
            .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

        let leaf_node = initial_art_record
            .art
            .get_node(NodeIndex::Index(self.index))?;
        if !leaf_node.is_leaf() {
            return Err(ApiError::BadRequest("The node isn't a leaf".to_string()));
        }

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(self.nonce);
        msg.extend(self.index.to_le_bytes());

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: self.signature,
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
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VerifyOwnershipQueryHelper {
    chat_id: Uuid,
    nonce: Vec<u8>,
    #[serde(with = "as_base64")]
    signature: Vec<u8>,
    sequence_number: Option<i64>,
}

impl From<DeleteChatQuery> for VerifyOwnershipQueryHelper {
    fn from(query: DeleteChatQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            nonce: query.nonce,
            signature: query.signature,
            sequence_number: None,
        }
    }
}

impl TryFrom<&[u8]> for VerifyOwnershipQueryHelper {
    type Error = ApiError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match serde_json::from_slice::<DeleteChatQuery>(value) {
            Ok(query) => Ok(Self::from(query)),
            Err(err) => Err(ApiError::BadRequest(err.to_string())),
        }
    }
}

impl VerifyOwnershipQueryHelper {
    // temporary set for testing
    pub async  fn verify(self, state: Arc<Container>) -> Result<(), ApiError> {
        Ok(())
    }

    // The real verification
    pub async fn _verify(self, state: Arc<Container>) -> Result<(), ApiError> {
        let previous_art_record = state
            .art_service
            .get_previous_art(&self.chat_id, self.sequence_number)
            .await?;

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(self.nonce);

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: self.signature,
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
}
