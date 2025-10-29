use ark_serialize::CanonicalDeserialize;
use cortado::CortadoAffine;
use zrt_art::art::art_types::PublicZeroArt;
use zrt_art::changes::branch_change::{BranchChange};
use zrt_art::changes::VerifiableChange;
use zrt_zk::art::ArtProof;
use zrt_zk::EligibilityRequirement;
use types::callback_wrappers::ProofVerifierMessage;
use types::errors::VerificationError;

#[derive(Clone, Debug)]
pub enum VerificationOpcode {
    InitGroup,
    KeyUpdate,
    AddMember,
    RemoveMember,
    LeaveGroup,
    SendMessage,
    GetMessages,
    GetChanges,
    GetArt,
    AuthRequest,
    DeleteChat,
}

pub enum PublicInputs {
    ArtUpdateInput {
        change: BranchChange<CortadoAffine>,
        art: PublicZeroArt,
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
                    change, art, eligibility_requirement
                } = self.data.public_inputs
                else {
                    return Err(VerificationError::InvalidInput);
                };

                Ok(ProofVerifierMessage::ArtUpdate {
                    change,
                    art,
                    eligibility_requirement,
                    associated_data: self.data.associated_data,
                    proof: ArtProof::deserialize_compressed(&*self.data.proof)?,
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
