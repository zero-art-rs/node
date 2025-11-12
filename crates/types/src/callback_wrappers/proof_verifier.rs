use callbacks::{CallbackSender, CallbackWrapper};
use cortado::CortadoAffine;
use zrt_art::art::PublicZeroArt;
use zrt_art::changes::aggregations::AggregatedChange;
use zrt_art::changes::branch_change::BranchChange;
use zrt_zk::EligibilityRequirement;
use zrt_zk::art::ArtProof;

pub enum ProofVerifierMessage {
    ArtUpdate {
        change: BranchChange<CortadoAffine>,
        art: PublicZeroArt<CortadoAffine>,
        associated_data: Vec<u8>,
        eligibility_requirement: EligibilityRequirement,
        proof: ArtProof,
    },
    ArtAggregation {
        change: AggregatedChange<CortadoAffine>,
        art: PublicZeroArt<CortadoAffine>,
        associated_data: Vec<u8>,
        eligibility_requirement: EligibilityRequirement,
        proof: ArtProof,
    },
    SchnorrSignature {
        signature: Vec<u8>,
        public_keys: Vec<CortadoAffine>,
        msg: Vec<u8>,
    },
}

#[derive(Debug, Clone)]
pub enum ProofVerifierResult {
    ArtUpdate { verdict: bool },
    ArtAggregation { verdict: bool },
    SchnorrSignature { verdict: bool },
}

pub type ProofVerifierMessageWrapper =
    CallbackWrapper<ProofVerifierMessage, eyre::Result<ProofVerifierResult>>;

pub type ProofVerifierCallbackSender = CallbackSender<eyre::Result<ProofVerifierResult>>;
