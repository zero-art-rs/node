use crate::domains::art::transport::http::{
    AddMemberRequest, RemoveMemberRequest, UpdateKeyRequest,
};
use crate::verification::VerificationError;
use crate::{Container, as_base64};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::traits::ARTPublicAPI;
use art::types::{BranchChanges, BranchChangesType};
use callbacks::callback;
use cortado::CortadoAffine;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio_util::bytes::Buf;
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;
use zk::art::ARTProof;

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
    pub async fn verify(&self, state: Arc<Container>) -> Result<(), VerificationError> {
        let branch_changes = BranchChanges::<CortadoAffine>::deserialize(&self.branch_changes)?;

        match branch_changes.change_type {
            BranchChangesType::UpdateKey => self.verify_update_key(state.clone()).await,
            BranchChangesType::AppendNode(_) => self.verify_add_member(state.clone()).await,
            BranchChangesType::MakeBlank(_, _) => self.verify_make_blank(state.clone()).await,
            _ => Err(VerificationError::UnsupportedOperation),
        }
    }

    async fn verify_update_key(&self, state: Arc<Container>) -> Result<(), VerificationError> {
        let branch_changes = BranchChanges::<CortadoAffine>::deserialize(&self.branch_changes)?;
        let mut art = state.art_service.get_art(&self.chat_id, None).await?.art;
        let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

        let proof = Self::update_proof(
            &self.proof,
            vec![art.get_node(branch_changes.node_index)?.public_key],
        )?;

        let key_update_message = ProofVerifierMessage::ArtUpdate {
            proof,
            co_path,
            associated_data: art.serialize()?,
        };

        let ProofVerifierResult::ArtUpdate { verdict } =
            callback(&state.proof_verifier_sender, key_update_message).await?
        else {
            return Err(VerificationError::InvalidResultMessage);
        };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }

    async fn verify_add_member(&self, state: Arc<Container>) -> Result<(), VerificationError> {
        let branch_changes = BranchChanges::<CortadoAffine>::deserialize(&self.branch_changes)?;
        let art = state.art_service.get_art(&self.chat_id, None).await?.art;
        let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

        let add_member_message = ProofVerifierMessage::ArtUpdate {
            proof: self.proof.clone(),
            co_path,
            associated_data: art.serialize()?,
        };

        let ProofVerifierResult::ArtUpdate { verdict } =
            callback(&state.proof_verifier_sender, add_member_message).await?
        else {
            return Err(VerificationError::InvalidResultMessage);
        };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }

    async fn verify_make_blank(&self, state: Arc<Container>) -> Result<(), VerificationError> {
        let branch_changes = BranchChanges::<CortadoAffine>::deserialize(&self.branch_changes)?;
        let art = state.art_service.get_art(&self.chat_id, None).await?.art;
        let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

        let remove_member_message = ProofVerifierMessage::ArtUpdate {
            proof: self.proof.clone(),
            co_path,
            associated_data: art.serialize()?,
        };

        let ProofVerifierResult::ArtUpdate { verdict } =
            callback(&state.proof_verifier_sender, remove_member_message).await?
        else {
            return Err(VerificationError::InvalidResultMessage);
        };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }

    fn update_proof(
        proof: &Vec<u8>,
        new_r: Vec<CortadoAffine>,
    ) -> Result<Vec<u8>, VerificationError> {
        let mut proof = ARTProof::deserialize_uncompressed(proof.reader())?;

        proof.R = new_r;
        let mut serialized_proof = Vec::new();
        proof.serialize_uncompressed(&mut serialized_proof)?;

        Ok(serialized_proof)
    }
}
