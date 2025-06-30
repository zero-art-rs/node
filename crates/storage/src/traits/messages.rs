use crate::DataStorage;
use mongodb::bson::{doc, from_document, DateTime, Document, Uuid};
use serde::de::DeserializeOwned;
use serde::Serialize;
use types::Message;

#[async_trait::async_trait]
pub trait MessageStorage: Send + Sync + DataStorage {
    async fn store_as_latest(
        &self,
        content: Vec<u8>,
        sender: String,
    ) -> Result<(), mongodb::error::Error>;
}
