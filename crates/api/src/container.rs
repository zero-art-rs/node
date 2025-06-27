use std::sync::Arc;

use crate::domains::{auth::service::AuthService, messenger::service::MessengerService};

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub auth_service: Arc<AuthService>,
}
