use callbacks::{CallbackSender, CallbackWrapper};
use cortado::CortadoAffine;

#[derive(Debug, Clone)]
pub enum ProofVerifierMessage {
    ArtUpdate {
        associated_data: Vec<u8>,
        aux_public_keys: Vec<CortadoAffine>,
        path: Vec<CortadoAffine>,
        co_path: Vec<CortadoAffine>,
        proof: Vec<u8>,
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
