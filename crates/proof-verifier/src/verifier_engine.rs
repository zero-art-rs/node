use crate::ProofVerifierSender;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::traits::{ARTPublicAPI, ARTPublicView};
use art::types::Direction;
use art::types::{BranchChanges, BranchChangesType, LeafIterWithPath, NodeIndex, PublicART};
use callbacks::callback;
use cortado::CortadoAffine;
use tokio_util::bytes::Buf;
use tracing::{debug, error};
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use types::errors::VerificationError;
use types::{art_schemas::*, centrifugo_schemas::AuthRequest, messenger_schemas::*};
use uuid::Uuid;


pub enum VerificationOpcode {
    KeyUpdate,
    AddMember,
    MakeBlank,
    SendMessage,
    GetMessages,
    GetChanges,
    AuthRequest,
    DeleteGroup,
}

pub enum PublicInputs {
    ArtUpdateInput {
        aux_public_keys: Vec<CortadoAffine>,
        path: Vec<CortadoAffine>,
        co_path: Vec<CortadoAffine>,
    },
    Signature {
        public_keys: Vec<CortadoAffine>,
    }
}

pub struct VerifierData {
    pub proof: Vec<u8>,
    pub public_inputs: PublicInputs,
    pub context: Vec<u8>,
}

pub struct VerificationRequest {
    pub opcode: VerificationOpcode,
    pub data: VerifierData,
}

impl VerificationRequest {
    pub fn create_message(&self) -> Result<ProofVerifierMessage, VerificationError> {
        match self.opcode {
            VerificationOpcode::KeyUpdate
            | VerificationOpcode::AddMember
            | VerificationOpcode::MakeBlank => {
                let PublicInputs::ArtUpdateInput {path, co_path, aux_public_keys} = &self.data.public_inputs
                else {
                    return Err(VerificationError::InvalidInput)
                };

                Ok(ProofVerifierMessage::ArtUpdate {
                    proof: self.data.proof.clone(),
                    co_path: co_path.clone(),
                    associated_data: self.data.context.clone(),
                    aux_public_keys: aux_public_keys.clone(),
                    path: path.clone(),
                })
            },
            _ => {
                let PublicInputs::Signature { public_keys } = &self.data.public_inputs
                else {
                    return Err(VerificationError::InvalidInput);
                };

                Ok(ProofVerifierMessage::SchnorrSignature {
                    signature: self.data.proof.clone(),
                    public_keys: public_keys.clone(),
                    msg: self.data.context.clone(),
                })
            }
        }
    }

    pub async fn verify(
        &self,
        proof_verifier_sender: &ProofVerifierSender
    ) -> Result<(), VerificationError> {
        let message = self.create_message()?;

        let verdict = match message {
            ProofVerifierMessage::ArtUpdate {..} => {
                match callback(proof_verifier_sender, message).await? {
                    ProofVerifierResult::ArtUpdate { verdict } => verdict,
                    _ => return Err(VerificationError::InvalidResultMessage)
                }
            },
            ProofVerifierMessage::SchnorrSignature {..} => {
                match callback(proof_verifier_sender, message).await? {
                    ProofVerifierResult::SchnorrSignature { verdict } => verdict,
                    _ => return Err(VerificationError::InvalidResultMessage)
                }
            }
        };

        match verdict {
            true => Ok(()),
            false => Err(VerificationError::InvalidProof),
        }
    }
}