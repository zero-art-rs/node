use cortado::CortadoAffine;
use types::callback_wrappers::{ProofVerifierMessage};
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

#[derive(Clone, Debug)]
pub enum PublicInputs {
    ArtUpdateInput {
        aux_public_keys: Vec<CortadoAffine>,
        path: Vec<CortadoAffine>,
        co_path: Vec<CortadoAffine>,
    },
    Signature {
        public_keys: Vec<CortadoAffine>,
    },
}

#[derive(Clone, Debug)]
pub struct VerifierData {
    pub proof: Vec<u8>,
    pub public_inputs: PublicInputs,
    pub context: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct VerificationRequest {
    pub opcode: VerificationOpcode,
    pub data: VerifierData,
}

impl VerificationRequest {
    pub fn to_message(self) -> Result<ProofVerifierMessage, VerificationError> {
        match self.opcode {
            VerificationOpcode::KeyUpdate
            | VerificationOpcode::AddMember
            | VerificationOpcode::RemoveMember => {
                let PublicInputs::ArtUpdateInput {
                    path,
                    co_path,
                    aux_public_keys,
                } = self.data.public_inputs
                else {
                    return Err(VerificationError::InvalidInput);
                };

                Ok(ProofVerifierMessage::ArtUpdate {
                    proof: self.data.proof,
                    co_path,
                    associated_data: self.data.context,
                    aux_public_keys,
                    path,
                })
            }
            _ => {
                let PublicInputs::Signature { public_keys } = self.data.public_inputs else {
                    return Err(VerificationError::InvalidInput);
                };

                Ok(ProofVerifierMessage::SchnorrSignature {
                    signature: self.data.proof,
                    public_keys,
                    msg: self.data.context,
                })
            }
        }
    }
}
