use crate::DATABASE;
use futures_util::TryStreamExt;
use mongodb::bson::Document;
use mongodb::error::Error;
use mongodb::Collection;
use serde::de::DeserializeOwned;
use serde::Serialize;
use types::ARTChangesRecord;

#[async_trait::async_trait]
pub trait DataStorage: Send + Sync {
    type Data: Send + Sync + Serialize + DeserializeOwned;

    async fn get_collection(&self) -> &'async_trait Collection<Self::Data>;

    async fn list(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Self::Data>, mongodb::error::Error> {
        let mut cursor = self
            .get_collection()
            .await
            .find(filter)
            .skip(skip as u64)
            .limit(limit)
            .await?;

        let mut records = Vec::new();
        while cursor.advance().await? {
            records.push(cursor.deserialize_current()?);
        }

        Ok(records)
    }

    async fn find_one(
        &self,
        filter: Document,
    ) -> Result<Option<Self::Data>, mongodb::error::Error> {
        let cursor = self.get_collection().await.find_one(filter).await?;

        Ok(cursor)
    }

    async fn store(&self, record: Self::Data) -> Result<(), mongodb::error::Error> {
        self.get_collection().await.insert_one(record).await?;

        Ok(())
    }

    async fn store_many(&self, records: Vec<Self::Data>) -> Result<(), mongodb::error::Error> {
        self.get_collection().await.insert_many(records).await?;

        Ok(())
    }

    async fn delete(&self, filter: Document) -> Result<Vec<Self::Data>, mongodb::error::Error> {
        let collection = &self.get_collection().await;
        let mut collection_cursor = collection.find(filter.clone()).await?;
        let mut records = Vec::new();

        while let Some(message) = collection_cursor.try_next().await? {
            records.push(message);
        }

        collection.delete_many(filter).await?;

        Ok(records)
    }

    async fn drop_collection(&self) -> Result<(), mongodb::error::Error> {
        self.get_collection().await.drop().await?;

        Ok(())
    }
}
