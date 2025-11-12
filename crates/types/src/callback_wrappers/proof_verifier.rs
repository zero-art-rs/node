use callbacks::{CallbackSender, CallbackWrapper};
use cortado::CortadoAffine;
use zrt_art::art::PublicZeroArt;
use zrt_art::changes::branch_change::BranchChange;
use zrt_zk::EligibilityRequirement;
use zrt_zk::art::ArtProof;

pub enum ProofVerifierMessage {
    // ArtUpdate {
    //     associated_data: Vec<u8>,
    //     aux_public_keys: Vec<CortadoAffine>,
    //     path: Vec<CortadoAffine>,
    //     co_path: Vec<CortadoAffine>,
    //     proof: Vec<u8>,
    // },
    ArtUpdate {
        change: BranchChange<CortadoAffine>,
        art: PublicZeroArt<CortadoAffine>,
        associated_data: Vec<u8>,
        eligibility_requirement: EligibilityRequirement,
        proof: ArtProof,
        // aux_public_keys: Vec<CortadoAffine>,
        // path: Vec<CortadoAffine>,
        // co_path: Vec<CortadoAffine>,
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
    SchnorrSignature { verdict: bool },
}

pub type ProofVerifierMessageWrapper =
    CallbackWrapper<ProofVerifierMessage, eyre::Result<ProofVerifierResult>>;

pub type ProofVerifierCallbackSender = CallbackSender<eyre::Result<ProofVerifierResult>>;
