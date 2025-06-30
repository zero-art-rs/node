use crate::DataStorage;
use mongodb::bson::{doc, from_document, DateTime, Document, Uuid};
use serde::de::DeserializeOwned;
use serde::Serialize;
use mongodb::{
    change_stream::{event::ChangeStreamEvent, ChangeStream},
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use types::Message;

#[async_trait::async_trait]
pub trait MessageStorage: Send + Sync {
    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<Message>>, mongodb::error::Error>;
    async fn store_message(
        &self,
        content: Vec<u8>,
        sender: String,
    ) -> Result<(), mongodb::error::Error>;
}
