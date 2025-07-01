use std::sync::Arc;

use crate::domains::{
    centrifugo::service::CentrifugoService,
    messenger::service::MessengerService,
    art::service::ARTService,
    invitation::service::InvitationService,
};

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub art_service: Arc<ARTService>,
    pub invitation_service: Arc<InvitationService>,
}
