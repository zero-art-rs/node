use mongodb::bson::oid::ObjectId;
use mongodb::bson::DateTime;
use types::Message;

#[async_trait::async_trait]
pub trait MessageStorage: Send + Sync {
    async fn store_message(&self, message: Message) -> Result<(), mongodb::error::Error>;
    async fn get_message(
        &self,
        created_at: &DateTime,
    ) -> Result<Option<Message>, mongodb::error::Error>;
    async fn get_message_by_id(
        &self,
        id: &ObjectId,
    ) -> Result<Option<Message>, mongodb::error::Error>;
    async fn list_messages(
        &self,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Message>, mongodb::error::Error>;
    async fn delete_message(
        &self,
        created_at: &DateTime,
    ) -> Result<Option<Message>, mongodb::error::Error>;

    async fn delete_message_by_id(
        &self,
        id: &ObjectId,
    ) -> Result<Option<Message>, mongodb::error::Error>;
}
