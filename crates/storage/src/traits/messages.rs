use mongodb::{
    bson::Document,
    change_stream::{event::ChangeStreamEvent, ChangeStream},
};
use types::Message;

#[async_trait::async_trait]
pub trait MessageStorage: Send + Sync {
    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<Message>>, mongodb::error::Error>;
    async fn store_message(
        &self,
        message: String,
        sender: String,
    ) -> Result<(), mongodb::error::Error>;
    async fn list_messages(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Message>, mongodb::error::Error>;
    async fn delete_messages(
        &self,
        filter: Document,
    ) -> Result<Vec<Message>, mongodb::error::Error>;
}
