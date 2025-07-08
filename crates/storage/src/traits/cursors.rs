use crate::{DataStorage, StorageError};
use types::CursorRecord;

#[async_trait::async_trait]
pub trait CursorStorage: Send + Sync + DataStorage {
    async fn update_user_cursor(
        &self,
        user_id: &str,
        sequence_number: i64,
    ) -> Result<Option<CursorRecord>, StorageError>;
}
