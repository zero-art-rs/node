use callbacks::{CallbackSender, CallbackWrapper};
use cortado::CortadoAffine;

#[derive(Debug, Clone)]
pub enum ProofVerifierMessage {
    AddMember {
        proof: Vec<u8>,
        co_path: Vec<CortadoAffine>,
        associated_data: Vec<u8>,
    },
    KeyUpdate {
        proof: Vec<u8>,
        co_path: Vec<CortadoAffine>,
        associated_data: Vec<u8>,
    },
    RemoveMember {
        proof: Vec<u8>,
        co_path: Vec<CortadoAffine>,
        associated_data: Vec<u8>,
    },
    SchnorrSignature {
        signature: Vec<u8>,
        public_keys: Vec<CortadoAffine>,
        msg: Vec<u8>,
    },
}

#[derive(Debug, Clone)]
pub enum ProofVerifierResult {
    AddMember { verdict: bool },
    ModifyArt { verdict: bool },
    KeyUpdate { verdict: bool },
    RemoveMember { verdict: bool },
    InitGroup { verdict: bool },
    SchnorrSignature { verdict: bool },
}

pub type ProofVerifierMessageWrapper =
    CallbackWrapper<ProofVerifierMessage, eyre::Result<ProofVerifierResult>>;

pub type ProofVerifierCallbackSender = CallbackSender<eyre::Result<ProofVerifierResult>>;
