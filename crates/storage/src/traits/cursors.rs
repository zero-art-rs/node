use crate::DataStorage;
use mongodb::bson::{doc, from_document, DateTime, Document, Uuid};
use types::{CursorRecord, Message};

#[async_trait::async_trait]
pub trait CursorStorage: Send + Sync + DataStorage {
    async fn update_user_cursor(
        &self,
        user_id: &str,
        sequence_number: i64,
    ) -> Result<Option<CursorRecord>, mongodb::error::Error>;
}
