use callbacks::{CallbackSender, CallbackWrapper};
use cortado::CortadoAffine;
use zrt_zk::EligibilityRequirement;
use zrt_zk::aggregated_art::VerifierAggregationTree;
use zrt_zk::art::{ArtProof, VerifierNodeData};

pub enum ProofVerifierMessage {
    ArtUpdate {
        verification_branch: Vec<VerifierNodeData<CortadoAffine>>,
        associated_data: Vec<u8>,
        eligibility_requirement: EligibilityRequirement,
        proof: ArtProof,
    },
    ArtAggregation {
        verification_tree: VerifierAggregationTree<CortadoAffine>,
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
