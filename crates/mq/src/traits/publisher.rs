use serde::{ Serialize };
use uuid::Uuid;

use crate::{
    errors::MqError
};

#[async_trait::async_trait]
pub trait Publisher: Send + Sync {
    async fn send_message<T>(
        &self, chat_id: Uuid, message: T
    ) -> Result<(), MqError>
    where
        T: Serialize + Send;
}