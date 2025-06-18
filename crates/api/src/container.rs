use mongodb::bson::Uuid;
use std::sync::Arc;

use storage::MongoMessageStorage;

use crate::domains::messenger::service::MessengerService;

pub struct Container {
    pub messenger_service: Arc<MessengerService<MongoMessageStorage>>,
}

impl Container {
    pub async fn get_messenger_service(
        &self,
        chat_id: Uuid,
    ) -> Result<Arc<MessengerService<MongoMessageStorage>>, mongodb::error::Error> {
        let message_storage = MongoMessageStorage::new(chat_id).await?;
        let messenger_service = MessengerService::new(message_storage);
        Ok(Arc::new(messenger_service))
    }
}
