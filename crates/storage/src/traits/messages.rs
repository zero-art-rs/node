use mongodb::bson::oid::ObjectId;
use mongodb::bson::{doc, from_document, DateTime, Document, Uuid};
use types::{CursorRecord, Message};

#[async_trait::async_trait]
pub trait MessageStorage: Send + Sync {
    async fn store_message(&self, message: String) -> Result<(), mongodb::error::Error>;
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
    async fn update_user_cursor(
        &self,
        user_id: &str,
        sequence_number: i64,
    ) -> Result<Option<CursorRecord>, mongodb::error::Error>;
    async fn list_cursors(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<CursorRecord>, mongodb::error::Error>;
    async fn delete_cursors(
        &self,
        filter: Document
    ) -> Result<Vec<CursorRecord>, mongodb::error::Error>;
}
