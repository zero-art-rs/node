use crate::domains::art::transport::http::{
    AddMemberRequest, RemoveMemberRequest, UpdateKeyRequest,
};
use crate::domains::art::transport::utils::decode_branch_changes;
use crate::errors::ApiError;
use crate::{Container, as_base64};
use art::traits::ARTPublicAPI;
use art::types::BranchChangesType;
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
pub struct ArtUpdateHelper {
    /// Serialized BranchChanges structure
    #[serde(with = "as_base64")]
    pub branch_changes: Vec<u8>,

    /// Serialized proof.
    #[serde(with = "as_base64")]
    pub proof: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    pub chat_id: Uuid,
}

impl From<AddMemberRequest> for ArtUpdateHelper {
    fn from(add_member_request: AddMemberRequest) -> Self {
        Self {
            branch_changes: add_member_request.branch_changes,
            proof: add_member_request.proof,
            chat_id: add_member_request.chat_id,
        }
    }
}

impl From<UpdateKeyRequest> for ArtUpdateHelper {
    fn from(add_member_request: UpdateKeyRequest) -> Self {
        Self {
            branch_changes: add_member_request.branch_changes,
            proof: add_member_request.proof,
            chat_id: add_member_request.chat_id,
        }
    }
}

impl From<RemoveMemberRequest> for ArtUpdateHelper {
    fn from(add_member_request: RemoveMemberRequest) -> Self {
        Self {
            branch_changes: add_member_request.branch_changes,
            proof: add_member_request.proof,
            chat_id: add_member_request.chat_id,
        }
    }
}

impl ArtUpdateHelper {
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
