use std::sync::Arc;

use storage::MongoMessageStorage;

use crate::domains::messenger::service::MessengerService;

pub struct Container {
    pub messenger_service: Arc<MessengerService<MongoMessageStorage>>,
}
