use ark_serialize::CanonicalDeserialize;
use cortado::CortadoAffine;
use tracing::error;
use types::callback_wrappers::ProofVerifierMessage;
use types::errors::VerificationError;
use zrt_art::art::PublicArt;
use zrt_art::changes::aggregations::AggregatedChange;
use zrt_art::changes::branch_change::BranchChange;
use zrt_zk::EligibilityRequirement;
use zrt_zk::art::ArtProof;

#[derive(Clone, Debug)]
pub enum VerificationOpcode {
    InitGroup,
    KeyUpdate,
    AddMember,
    RemoveMember,
    LeaveGroup,
    Aggregation,
    SendMessage,
    GetMessages,
    GetChanges,
    GetArt,
    AuthRequest,
    DeleteChat,
}

#[derive(Debug)]
pub enum PublicInputs {
    ArtUpdateInput {
        change: BranchChange<CortadoAffine>,
        art: PublicArt<CortadoAffine>,
        eligibility_requirement: EligibilityRequirement,
    },
    ArtAggregationInput {
        change: AggregatedChange<CortadoAffine>,
        art: PublicArt<CortadoAffine>,
        eligibility_requirement: EligibilityRequirement,
    },
    Signature {
        public_keys: Vec<CortadoAffine>,
    },
}

pub struct VerifierData {
    pub proof: Vec<u8>,
    pub public_inputs: PublicInputs,
    pub associated_data: Vec<u8>,
}

pub struct VerificationRequest {
    pub opcode: VerificationOpcode,
    pub data: VerifierData,
}

impl VerificationRequest {
    pub fn to_message(self) -> Result<ProofVerifierMessage, VerificationError> {
        match self.opcode {
            VerificationOpcode::KeyUpdate
            | VerificationOpcode::LeaveGroup
            | VerificationOpcode::AddMember
            | VerificationOpcode::RemoveMember => {
                let PublicInputs::ArtUpdateInput {
                    change,
                    art,
                    eligibility_requirement,
                } = self.data.public_inputs
                else {
                    return Err(VerificationError::InvalidInput);
                };

                Ok(ProofVerifierMessage::ArtUpdate {
                    verification_branch: art.verification_branch(&change)?,
                    eligibility_requirement,
                    associated_data: self.data.associated_data,
                    proof: ArtProof::deserialize_compressed(&*self.data.proof)?,
                })
            }
            VerificationOpcode::Aggregation => {
                let PublicInputs::ArtAggregationInput {
                    change,
                    art,
                    eligibility_requirement,
                } = self.data.public_inputs
                else {
                    return Err(VerificationError::InvalidInput);
                };

                let deserialized_proof = ArtProof::deserialize_compressed(&*self.data.proof);
                if deserialized_proof.is_err() {
                    error!("Failed to deserialize proof");
                }
                let deserialized_proof = deserialized_proof?;

                Ok(ProofVerifierMessage::ArtAggregation {
                    verification_tree: art.verification_tree(&change)?,
                    eligibility_requirement,
                    associated_data: self.data.associated_data,
                    proof: deserialized_proof,
                })
            }
            _ => {
                let PublicInputs::Signature { public_keys } = self.data.public_inputs else {
                    return Err(VerificationError::InvalidInput);
                };

                Ok(ProofVerifierMessage::SchnorrSignature {
                    signature: self.data.proof,
                    public_keys,
                    msg: self.data.associated_data,
                })
            }
        }
    }
}
