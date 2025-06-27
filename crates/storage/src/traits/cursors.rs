use mongodb::bson::Document;
use types::CursorRecord;

#[async_trait::async_trait]
pub trait CursorStorage: Send + Sync {
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
        filter: Document,
    ) -> Result<Vec<CursorRecord>, mongodb::error::Error>;
}
