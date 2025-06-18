use mongodb::bson::oid::ObjectId;
use mongodb::bson::{doc, from_document, DateTime, Uuid};
use types::{CursorRecord, Message};

#[async_trait::async_trait]
pub trait MessageStorage: Send + Sync {
    async fn store_message(&self, message: String) -> Result<(), mongodb::error::Error>;
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
    async fn list_cursors(
        &self,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<CursorRecord>, mongodb::error::Error>;
    async fn delete_message(
        &self,
        created_at: &DateTime,
    ) -> Result<Option<Message>, mongodb::error::Error>;

    async fn delete_message_by_id(
        &self,
        id: &ObjectId,
    ) -> Result<Option<Message>, mongodb::error::Error>;

    async fn update_user_cursor(
        &self,
        user_id: &str,
        sequence_number: i64,
    ) -> Result<Option<CursorRecord>, mongodb::error::Error>;
}
