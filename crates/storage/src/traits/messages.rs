use mongodb::bson::Document;
use mongodb::bson::oid::ObjectId;
use serde::Serialize;

#[async_trait::async_trait]
pub trait MessageStorage: Send + Sync {
    async fn store_message<T>(&self, message: T) -> Result<(), mongodb::error::Error>
    where
        T: Serialize + Send;
    async fn get_message(&self, id: &ObjectId) -> Result<Option<Document>, mongodb::error::Error>;
    async fn list_messages(
        &self,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Document>, mongodb::error::Error>;
    async fn delete_message(&self, id: &ObjectId) -> Result<Option<Document>, mongodb::error::Error>;
}
