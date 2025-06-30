use std::sync::Arc;

use proof_verifier::ProofVerifierSender;

use crate::domains::{
    art::service::ARTService, centrifugo::service::CentrifugoService,
    invitation::service::InvitationService, messenger::service::MessengerService,
};

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub art_service: Arc<ARTService>,
    pub invitation_service: Arc<InvitationService>,

    pub proof_verifier_sender: ProofVerifierSender,
}
