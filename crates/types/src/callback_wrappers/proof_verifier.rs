use callbacks::{CallbackSender, CallbackWrapper};
use cortado::CortadoAffine;
use std::fmt::Debug;
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

impl Debug for ProofVerifierMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProofVerifierMessage::ArtUpdate {
                verification_branch,
                associated_data,
                eligibility_requirement,
                ..
            } => f
                .debug_struct("ArtUpdate")
                .field("verification_branch", &verification_branch)
                .field("associated_data", &associated_data)
                .field("eligibility_requirement", &eligibility_requirement)
                .finish(),
            ProofVerifierMessage::ArtAggregation {
                verification_tree,
                associated_data,
                eligibility_requirement,
                ..
            } => f
                .debug_struct("ArtAggregation")
                .field("verification_tree", &verification_tree)
                .field("associated_data", &associated_data)
                .field("eligibility_requirement", &eligibility_requirement)
                .finish(),
            ProofVerifierMessage::SchnorrSignature {
                signature,
                public_keys,
                msg,
            } => f
                .debug_struct("SchnorrSignature")
                .field("signature", &signature)
                .field("public_keys", &public_keys)
                .field("msg", &msg)
                .finish(),
        }
    }
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
