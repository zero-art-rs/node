use std::sync::Arc;

use proof_verifier::ProofVerifierSender;

use crate::domains::{
    centrifugo::service::CentrifugoService, messenger::service::MessengerService,
};

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,

    pub proof_verifier_sender: ProofVerifierSender,
}
