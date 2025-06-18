use std::sync::Arc;

use storage::MongoMessageStorage;

use crate::domains::{auth::service::AuthService, messenger::service::MessengerService};

pub struct Container {
    pub messenger_service: Arc<MessengerService<MongoMessageStorage>>,
    pub auth_service: Arc<AuthService>,
}
