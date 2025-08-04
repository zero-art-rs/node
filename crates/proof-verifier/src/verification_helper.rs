use crate::ProofVerifierSender;
use ark_serialize::CanonicalDeserialize;
use art::traits::{ARTPublicAPI, ARTPublicView};
use art::types::{BranchChanges, BranchChangesType, Direction, NodeIndex, NodeIterWithPath, PublicART};
use callbacks::callback;
use cortado::CortadoAffine;
use tokio_util::bytes::Buf;
use tracing::{error, info};
use types::art_schemas::*;
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use types::errors::VerificationError;
use types::messenger_schemas::*;
use uuid::Uuid;
use zk::art::ARTProof;

pub struct VerificationHelper {
    pub chat_id: Uuid,
    pub helper_type: HelperType,
    pub sequence_number: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum HelperType {
    ArtUpdate {
        branch_changes: Vec<u8>,
        proof: Vec<u8>,
    },
    LeafKnowledge {
        nonce: Vec<u8>,
        index: u32,
        signature: Vec<u8>,
    },
    InvitePossession {
        nonce: Vec<u8>,
        signature: Vec<u8>,
        challenge: Vec<u8>,
        public_key: CortadoAffine,
    },
    Ownership {
        nonce: Vec<u8>,
        signature: Vec<u8>,
    },
    RootKnowledge {
        nonce: Vec<u8>,
        signature: Vec<u8>,
    },
}

impl HelperType {
    pub fn get_challenge(&self) -> Option<&Vec<u8>> {
        match self {
            Self::InvitePossession { challenge, .. } => Some(challenge),
            _ => None,
        }
    }

    pub fn set_challenge(&mut self, new_challenge: Vec<u8>) {
        match self {
            Self::InvitePossession { challenge, .. } => *challenge = new_challenge,
            _ => {}
        }
    }

    pub fn get_index(&self) -> Option<u32> {
        match self {
            Self::LeafKnowledge { index, .. } => Some(*index),
            _ => None,
        }
    }
}

impl From<AddMemberRequest> for VerificationHelper {
    fn from(request: AddMemberRequest) -> Self {
        Self {
            chat_id: request.chat_id,
            helper_type: HelperType::ArtUpdate {
                branch_changes: request.branch_changes,
                proof: request.proof,
            },
            sequence_number: None,
        }
    }
}

impl From<UpdateKeyRequest> for VerificationHelper {
    fn from(request: UpdateKeyRequest) -> Self {
        Self {
            chat_id: request.chat_id,
            helper_type: HelperType::ArtUpdate {
                branch_changes: request.branch_changes,
                proof: request.proof,
            },
            sequence_number: None,
        }
    }
}

impl From<RemoveMemberRequest> for VerificationHelper {
    fn from(request: RemoveMemberRequest) -> Self {
        Self {
            chat_id: request.chat_id,
            helper_type: HelperType::ArtUpdate {
                branch_changes: request.branch_changes,
                proof: request.proof,
            },
            sequence_number: None,
        }
    }
}

impl From<GetInitialARTQuery> for VerificationHelper {
    fn from(query: GetInitialARTQuery) -> Self {
        let public_key = CortadoAffine::deserialize_uncompressed(&*query.public_key).unwrap_or_else(|_| {
            info!("Failed to deserialize public key");
            CortadoAffine::default()
        });
        
        Self {
            chat_id: query.chat_id,
            helper_type: HelperType::InvitePossession {
                nonce: query.nonce,
                signature: query.signature,
                challenge: query.challenge,
                public_key,
            },
            sequence_number: None,
        }
    }
}

impl From<UpdateMetadataRequest> for VerificationHelper {
    fn from(query: UpdateMetadataRequest) -> Self {
        Self {
            chat_id: query.chat_id,
            helper_type: HelperType::LeafKnowledge {
                nonce: query.nonce,
                index: query.index,
                signature: query.signature,
            },
            sequence_number: None,
        }
    }
}

impl From<DeleteChatQuery> for VerificationHelper {
    fn from(query: DeleteChatQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            helper_type: HelperType::Ownership {
                nonce: query.nonce,
                signature: query.signature,
            },
            sequence_number: None,
        }
    }
}

impl From<SendMessageRequest> for VerificationHelper {
    fn from(query: SendMessageRequest) -> Self {
        Self {
            chat_id: query.chat_id,
            helper_type: HelperType::RootKnowledge {
                nonce: query.nonce,
                signature: query.signature,
            },
            sequence_number: None,
        }
    }
}

impl From<GetMessageQuery> for VerificationHelper {
    fn from(query: GetMessageQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            helper_type: HelperType::RootKnowledge {
                nonce: query.nonce,
                signature: query.signature,
            },
            sequence_number: query.sequence_number,
        }
    }
}

impl From<DeleteMessageQuery> for VerificationHelper {
    fn from(query: DeleteMessageQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            helper_type: HelperType::RootKnowledge {
                nonce: query.nonce,
                signature: query.signature,
            },
            sequence_number: query.sequence_number,
        }
    }
}

impl From<GetChangesQuery> for VerificationHelper {
    fn from(query: GetChangesQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            helper_type: HelperType::RootKnowledge {
                nonce: query.nonce,
                signature: query.signature,
            },
            sequence_number: query.sequence_number,
        }
    }
}

impl From<GetARTQuery> for VerificationHelper {
    fn from(query: GetARTQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            helper_type: HelperType::RootKnowledge {
                nonce: query.nonce,
                signature: query.signature,
            },
            sequence_number: query.sequence_number,
        }
    }
}

impl VerificationHelper {
    pub async fn verify(
        &self,
        art: &PublicART<CortadoAffine>,
        proof_verifier_sender: &ProofVerifierSender,
    ) -> Result<(), VerificationError> {
        match &self.helper_type {
            HelperType::ArtUpdate {
                branch_changes,
                proof,
            } => {
                self.verify_art_update(art, proof_verifier_sender, branch_changes, proof)
                    .await
            }
            HelperType::LeafKnowledge {
                nonce,
                index,
                signature,
            } => {
                self.verify_leaf_knowledge(
                    art,
                    proof_verifier_sender,
                    nonce,
                    *index,
                    signature,
                )
                .await
            }
            HelperType::InvitePossession {
                nonce,
                signature,
                challenge,
                public_key,
            } => {
                self.verify_invite_possession(
                    art,
                    proof_verifier_sender,
                    challenge.as_ref(),
                    nonce,
                    signature,
                    public_key.clone(),
                )
                    .await
            }
            HelperType::Ownership { nonce, signature } => {
                self.verify_ownership(art, proof_verifier_sender, nonce, signature)
                    .await
            }
            HelperType::RootKnowledge {
                nonce, signature, ..
            } => {
                self.verify_root_knowledge(art, proof_verifier_sender, nonce, signature)
                    .await
            }
        }
    }

    async fn verify_art_update(
        &self,
        art: &PublicART<CortadoAffine>,
        proof_verifier_sender: &ProofVerifierSender,
        branch_changes: &Vec<u8>,
        proof: &[u8],
    ) -> Result<(), VerificationError> {
        let mut art = art.clone();
        let branch_changes = BranchChanges::<CortadoAffine>::deserialize(branch_changes)?;

        match branch_changes.change_type {
            BranchChangesType::UpdateKey => Self::check_auxiliary_public_keys(
                proof,
                vec![art.get_node(&branch_changes.node_index)?.public_key],
            )?,
            BranchChangesType::AppendNode(_) => {
                Self::check_auxiliary_public_keys(proof, vec![art.root.public_key])?
            }
            BranchChangesType::MakeBlank(_, _) => {
                Self::check_auxiliary_public_keys(proof, vec![art.root.public_key])?
            }
            _ => return Err(VerificationError::UnsupportedOperation),
        };

        let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

        let key_update_message = ProofVerifierMessage::ArtUpdate {
            proof: proof.to_vec(),
            co_path,
            associated_data: PublicART::serialize(&art)?,
        };

        let ProofVerifierResult::ArtUpdate { verdict } =
            callback(proof_verifier_sender, key_update_message).await?
        else {
            return Err(VerificationError::InvalidResultMessage);
        };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }

    /// Deserializes and checks if auxiliary public key is correct,
    fn check_auxiliary_public_keys(
        proof: &[u8],
        new_r: Vec<CortadoAffine>,
    ) -> Result<(), VerificationError> {
        let proof_r = ARTProof::deserialize_uncompressed(proof.reader())?.R;

        if proof_r.len() != new_r.len() {
            return Err(VerificationError::InvalidProof);
        }

        for (a, b) in proof_r.iter().zip(new_r.iter()) {
            if a != b {
                return Err(VerificationError::InvalidProof);
            }
        }

        Ok(())
    }

    pub async fn verify_leaf_knowledge(
        &self,
        art: &PublicART<CortadoAffine>,
        proof_verifier_sender: &ProofVerifierSender,
        nonce: &Vec<u8>,
        index: u32,
        signature: &[u8],
    ) -> Result<(), VerificationError> {
        let art = art.clone();
        // Check if provided index maps to leaf node
        let leaf = art.get_node(&NodeIndex::Index(index))?;
        if !leaf.is_leaf() {
            return Err(VerificationError::InvalidProof);
        }

        // Compute transcript
        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(nonce);
        msg.extend(index.to_le_bytes());

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: signature.to_vec(),
            public_keys: vec![leaf.public_key],
            msg,
        };

        // Verify signature
        let verdict = match callback(proof_verifier_sender, schnorr_signature_message).await? {
            ProofVerifierResult::SchnorrSignature { verdict } => verdict,
            _ => return Err(VerificationError::InvalidResultMessage),
        };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }

    pub async fn verify_invite_possession(
        &self,
        art: &PublicART<CortadoAffine>,
        proof_verifier_sender: &ProofVerifierSender,
        challenge: &Vec<u8>,
        nonce: &Vec<u8>,
        signature: &[u8],
        public_key: CortadoAffine
    ) -> Result<(), VerificationError> {
        info!("Check if provided public key is correct");
        let mut public_key_is_wrong = true;
        for (node, path) in NodeIterWithPath::new(art.get_root()) {
            if node.public_key.eq(&public_key) && node.is_leaf(){
                public_key_is_wrong = false;
            }
        }

        if public_key_is_wrong {
            error!("The node corresponding to the provided public isn't leaf, or public key is incorrect");
            return Err(VerificationError::InvalidProof);
        }
        
        info!("Compute transcript");
        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(nonce);
        msg.extend(challenge);

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: signature.to_vec(),
            public_keys: vec![public_key],
            msg,
        };

        info!("Verifying proof");
        let verdict = match callback(proof_verifier_sender, schnorr_signature_message).await? {
            ProofVerifierResult::SchnorrSignature { verdict } => verdict,
            _ => return Err(VerificationError::InvalidResultMessage),
        };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }

    pub async fn verify_ownership(
        &self,
        art: &PublicART<CortadoAffine>,
        proof_verifier_sender: &ProofVerifierSender,
        nonce: &Vec<u8>,
        signature: &[u8],
    ) -> Result<(), VerificationError> {
        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(nonce);

        let mut left_most_leaf = &art.root;
        while let Ok(node) = left_most_leaf.get_left() {
            left_most_leaf = node;
        }

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: signature.to_vec(),
            public_keys: vec![left_most_leaf.public_key],
            msg,
        };

        let ProofVerifierResult::SchnorrSignature { verdict } =
            callback(proof_verifier_sender, schnorr_signature_message).await?
        else {
            return Err(VerificationError::InvalidResultMessage);
        };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }

    pub async fn verify_root_knowledge(
        &self,
        art: &PublicART<CortadoAffine>,
        proof_verifier_sender: &ProofVerifierSender,
        nonce: &Vec<u8>,
        signature: &[u8],
    ) -> Result<(), VerificationError> {
        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(nonce);

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: signature.to_vec(),
            public_keys: vec![art.root.public_key],
            msg,
        };

        let ProofVerifierResult::SchnorrSignature { verdict } =
            callback(proof_verifier_sender, schnorr_signature_message).await?
        else {
            return Err(VerificationError::InvalidResultMessage);
        };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }
}
