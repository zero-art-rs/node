use crate::ProofVerifierSender;
use ark_serialize::CanonicalDeserialize;
use art::traits::ARTPublicAPI;
use art::types::{BranchChanges, BranchChangesType, NodeIndex, PublicART};
use callbacks::callback;
use cortado::CortadoAffine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::bytes::Buf;
use tracing::info;
use types::art_schemas::*;
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use types::errors::VerificationError;
use types::messenger_schemas::*;
use types::utils::as_base64;
use uuid::Uuid;
use zk::art::ARTProof;

type ChallengeHashMap = HashMap<(Uuid, CortadoAffine), Vec<u8>>;

pub struct VerificationHelper {
    pub chat_id: Uuid,
    pub helper_type: HelperType,
    pub sequence_number: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum HelperType {
    ArtUpdate {
        #[serde(with = "as_base64")]
        branch_changes: Vec<u8>,
        #[serde(with = "as_base64")]
        proof: Vec<u8>,
    },
    GetInitialART {
        #[serde(with = "as_base64")]
        nonce: Vec<u8>,
        index: u32,
        #[serde(with = "as_base64")]
        signature: Vec<u8>,
    },
    Ownership {
        #[serde(with = "as_base64")]
        nonce: Vec<u8>,
        #[serde(with = "as_base64")]
        signature: Vec<u8>,
    },
    RootKnowledge {
        #[serde(with = "as_base64")]
        nonce: Vec<u8>,
        #[serde(with = "as_base64")]
        signature: Vec<u8>,
    },
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
        Self {
            chat_id: query.chat_id,
            helper_type: HelperType::GetInitialART {
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
        challenges: Arc<RwLock<ChallengeHashMap>>,
    ) -> Result<(), VerificationError> {
        match &self.helper_type {
            HelperType::ArtUpdate {
                branch_changes,
                proof,
            } => {
                self.verify_art_update(art, proof_verifier_sender, branch_changes, proof)
                    .await
            }
            HelperType::GetInitialART {
                nonce,
                index,
                signature,
            } => {
                self.verify_get_initial_art(
                    art,
                    proof_verifier_sender,
                    challenges,
                    nonce,
                    *index,
                    signature,
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
                vec![art.get_node(branch_changes.node_index.clone())?.public_key],
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

    pub async fn verify_get_initial_art(
        &self,
        art: &PublicART<CortadoAffine>,
        proof_verifier_sender: &ProofVerifierSender,
        challenges: Arc<RwLock<ChallengeHashMap>>,
        nonce: &Vec<u8>,
        index: u32,
        signature: &[u8],
    ) -> Result<(), VerificationError> {
        let mut art = art.clone();
        let leaf_node = art.get_node(NodeIndex::Index(index))?;
        if !leaf_node.is_leaf() {
            return Err(VerificationError::InvalidProof);
        }

        let challenge = match challenges
            .write()
            .await
            .remove(&(self.chat_id, leaf_node.public_key))
        {
            Some(challenge) => challenge.clone(),
            None => return Err(VerificationError::NoChallenge),
        };

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(nonce);
        msg.extend(index.to_le_bytes());
        msg.extend(challenge);

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: signature.to_vec(),
            public_keys: vec![leaf_node.public_key],
            msg,
        };

        let verdict = match callback(proof_verifier_sender, schnorr_signature_message).await? {
            ProofVerifierResult::SchnorrSignature { verdict } => {
                info!("Remove used challenge");

                verdict
            }
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
