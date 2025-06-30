use callbacks::{CallbackSender, CallbackWrapper};

#[derive(Debug, Clone)]
pub enum ProofVerifierMessage {
    AddMember {
        proof: Vec<u8>,
        // TODO: add operation specific data
    },
    ModifyArt {
        proof: Vec<u8>,
        // TODO: add operation specific data
    },
}

#[derive(Debug, Clone)]
pub enum ProofVerifierResult {
    AddMember { verdict: bool },
    ModifyArt { verdict: bool },
}

pub type ProofVerifierMessageWrapper =
    CallbackWrapper<ProofVerifierMessage, eyre::Result<ProofVerifierResult>>;

pub type ProofVerifierCallbackSender = CallbackSender<eyre::Result<ProofVerifierResult>>;
