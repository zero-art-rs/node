use mongodb::change_stream::{event::ChangeStreamEvent, ChangeStream};
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
