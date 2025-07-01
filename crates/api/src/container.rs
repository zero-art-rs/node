use mongodb::bson::Uuid;
use std::sync::Arc;

use crate::domains::art::service::ARTService;
use crate::domains::centrifugo::service::CentrifugoService;
use crate::domains::invitations::service::InvitationService;
use crate::domains::messenger::service::MessengerService;
use storage::MongoMessageStorage;

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub art_service: Arc<ARTService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub invitation_service: Arc<InvitationService>,
}
