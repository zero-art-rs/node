pub(crate) use crate::{DataStorage, MongoDataStorage};
use bson::doc;
use futures_util::TryStreamExt;
use mongodb::{bson::Document, ClientSession};
use serde::{Serialize};
use tracing::debug;
use mongodb::error::Error;
use serde::de::DeserializeOwned;

#[async_trait::async_trait]
impl<M, D> DataStorage<D, Error, ClientSession> for M
where
    M: MongoDataStorage<D>,
    D: Send + Sync + Serialize + DeserializeOwned + 'static,
{
    async fn list(
        &self,
        filter: Document,
        _sort_option: Option<Document>,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<D>, Error> {
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

    async fn count(&self, filter: Document, limit: i64, skip: i64) -> Result<u64, Error> {
        Ok(self
            .get_collection()
            .await
            .count_documents(filter)
            // .find(filter)
            .skip(skip as u64)
            .limit(limit as u64)
            .await?)
    }

    async fn find_one(&self, filter: Document) -> Result<Option<D>, Error> {
        let cursor = self.get_collection().await.find_one(filter).await?;

        Ok(cursor)
    }

    async fn find_one_and_replace(&self, filter: Document, replacement: D, session: Option<&mut ClientSession>) -> Result<Option<D>, Error> {
        let cursor = self.get_collection().await.find_one_and_replace(filter, replacement);
        
        let cursor = match session {
            Some(session) => cursor.session(session).await?,
            None => cursor.await?, 
        };

        Ok(cursor)
    }

    async fn insert_one(&self, record: D) -> Result<(), Error> {
        self.get_collection().await.insert_one(record).await?;

        Ok(())
    }

    async fn insert_many(&self, records: Vec<D>) -> Result<(), Error> {
        self.get_collection().await.insert_many(records).await?;

        Ok(())
    }

    async fn delete(&self, filter: Document) -> Result<Vec<D>, Error> {
        let collection = &self.get_collection().await;
        let mut collection_cursor = collection.find(filter.clone()).await?;
        let mut records = Vec::new();

        while let Some(message) = collection_cursor.try_next().await? {
            records.push(message);
        }

        collection.delete_many(filter).await?;

        Ok(records)
    }

    async fn delete_one(&self, filter: Document, session: Option<&mut ClientSession>) -> Result<(), Error> {
        let delete_one_request = self.get_collection().await.delete_one(filter);

        match session {
            Some(session) => delete_one_request.session(session).await?,
            None => delete_one_request.await?,
        };

        Ok(())
    }

    async fn clear(&self, session: &mut ClientSession) -> Result<(), Error> {
        debug!("Clear message collection ...");
        self.get_collection()
            .await
            .delete_many(doc! {})
            .session(session)
            .await?;

        debug!("Message collection cleared successfully");
        Ok(())
    }

    async fn drop_collection(&self) -> Result<(), Error> {
        self.get_collection().await.drop().await?;
        Ok(())
    }

    async fn drop_collection_in_session(
        &self,
        sesion: &mut ClientSession,
    ) -> Result<(), Error> {
        self.get_collection().await.drop().session(sesion).await?;
        Ok(())
    }

    async fn drop_collection_if_empty(&self) -> Result<(), Error> {
        let collection = self.get_collection().await;
        if collection.find_one(doc! {}).await?.is_none() {
            collection.drop().await?;
        }

        Ok(())
    }
}

