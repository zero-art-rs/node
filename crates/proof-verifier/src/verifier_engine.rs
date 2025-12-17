use ark_serialize::CanonicalDeserialize;
use cortado::CortadoAffine;
use tracing::{error, warn};
use types::ARTRecord;
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

impl VerifierData {
    pub fn new(proof: Vec<u8>, public_inputs: PublicInputs, associated_data: Vec<u8>) -> Self {
        Self {
            proof,
            public_inputs,
            associated_data,
        }
    }
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

pub struct PostVerificationData {
    epoch: u64,
    base_tk: CortadoAffine,
    upstream_tk: CortadoAffine,
}

impl PostVerificationData {
    pub fn new(epoch: u64, base_tk: CortadoAffine, upstream_tk: CortadoAffine) -> Self {
        Self {
            epoch,
            base_tk,
            upstream_tk,
        }
    }

    pub fn post_verify_data_frame(
        &self,
        art: &ARTRecord<CortadoAffine>,
    ) -> Result<(), VerificationError> {
        let epoch = art.epoch;
        let upstream_tk = art.art.preview().root().public_key();
        if self.epoch == epoch && self.upstream_tk == upstream_tk {
            Ok(())
        } else {
            warn!(
                used_epoch = ?self.epoch,
                current_epoch = ?art.epoch,
                used_base_tk = ?self.base_tk,
                current_base_tk = ?art.art.root().data().public_key(),
                "Fail to post verify, as the state already changed",
            );
            Err(VerificationError::FailedPostVerification)
        }
    }

    
    pub fn post_verify_update(
        &self,
        art: &ARTRecord<CortadoAffine>,
    ) -> Result<(), VerificationError> {
        if self.epoch == art.epoch
            && self.base_tk == art.art.root().data().public_key()
        {
            Ok(())
        } else if self.epoch == art.epoch + 1
            && self.upstream_tk == art.art.preview().root().public_key()
        {
            Ok(())
        } else {
            warn!(
                used_epoch = ?self.epoch,
                current_epoch = ?art.epoch,
                used_upstream_tk = ?self.upstream_tk,
                current_upstream_tk = ?art.art.preview().root().public_key(),
                "Fail to post verify, as the state already changed",
            );
            Err(VerificationError::FailedPostVerification)
        }
    }
}
