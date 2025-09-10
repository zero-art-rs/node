use crate::StorageError;
use bson::doc;
use futures_util::TryStreamExt;
use mongodb::options::FindOptions;
use mongodb::{bson::Document, ClientSession, Collection};
use serde::{de::DeserializeOwned, Serialize};
use tracing::debug;

#[async_trait::async_trait]
pub trait DataStorage: Send + Sync {
    type Data: Send + Sync + Serialize + DeserializeOwned;

    async fn get_collection(&self) -> &Collection<Self::Data>;

    async fn list(
        &self,
        filter: Document,
        sort_option: Option<Document>,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Self::Data>, StorageError> {
        let mut cursor = self
            .get_collection()
            .await
            .find(filter)
            // .sort(sort_option.unwrap_or_default())
            .skip(skip as u64)
            .limit(limit as i64)
            .await?;

        let mut records = Vec::new();
        while cursor.advance().await? {
            records.push(cursor.deserialize_current()?);
        }

        Ok(records)
    }

    async fn count(&self, filter: Document, limit: i64, skip: i64) -> Result<u64, StorageError> {
        Ok(self
            .get_collection()
            .await
            .count_documents(filter)
            // .find(filter)
            .skip(skip as u64)
            .limit(limit as u64)
            .await?)
    }

    async fn find_one(&self, filter: Document) -> Result<Option<Self::Data>, StorageError> {
        let cursor = self.get_collection().await.find_one(filter).await?;

        Ok(cursor)
    }

    async fn insert_one(&self, record: Self::Data) -> Result<(), StorageError> {
        self.get_collection().await.insert_one(record).await?;

        Ok(())
    }

    async fn insert_many(&self, records: Vec<Self::Data>) -> Result<(), StorageError> {
        self.get_collection().await.insert_many(records).await?;

        Ok(())
    }

    async fn delete(&self, filter: Document) -> Result<Vec<Self::Data>, StorageError> {
        let collection = &self.get_collection().await;
        let mut collection_cursor = collection.find(filter.clone()).await?;
        let mut records = Vec::new();

        while let Some(message) = collection_cursor.try_next().await? {
            records.push(message);
        }

        collection.delete_many(filter).await?;

        Ok(records)
    }

    async fn clear(&self, session: &mut ClientSession) -> Result<(), mongodb::error::Error> {
        debug!("Clear message collection ...");
        self.get_collection()
            .await
            .delete_many(doc! {})
            .session(session)
            .await?;

        debug!("Message collection cleared successfully");
        Ok(())
    }

    async fn drop_collection(&self) -> Result<(), mongodb::error::Error> {
        self.get_collection().await.drop().await?;
        Ok(())
    }

    async fn drop_collection_in_session(&self, sesion: &mut ClientSession) -> Result<(), mongodb::error::Error> {
        self.get_collection().await.drop().session(sesion).await?;
        Ok(())
    }

    async fn drop_collection_if_empty(&self) -> Result<(), mongodb::error::Error> {
        let collection = self.get_collection().await;
        if collection.find_one(doc! {}).await?.is_none() {
            collection.drop().await?;
        }

        Ok(())
    }
}
