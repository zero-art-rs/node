use crate::domains::art::service::ARTService;
use crate::domains::auth::service::AuthService;
use crate::domains::invitations::service::InvitationService;
use crate::domains::messenger::service::MessengerService;
use std::sync::Arc;

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub art_service: Arc<ARTService>,
    pub auth_service: Arc<AuthService>,
    pub invitation_service: Arc<InvitationService>,
}
