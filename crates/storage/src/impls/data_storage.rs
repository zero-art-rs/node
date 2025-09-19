pub(crate) use crate::{DataStorage, MongoDataStorage};
use bson::doc;
use futures_util::{StreamExt, TryStreamExt};
use mongodb::error::Error;
use mongodb::{bson::Document, ClientSession};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tracing::debug;
use crate::SessionSupport;

#[async_trait::async_trait]
impl<M, D> DataStorage<D, Error, ClientSession> for M
where
    M: MongoDataStorage<D>,
    D: Send + Sync + Serialize + DeserializeOwned + 'static,
{
    async fn list(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
        session: Option<&mut ClientSession>,
    ) -> Result<Vec<D>, Error> {
        let mut find_request = self
            .get_collection()
            .await
            .find(filter)
            .skip(skip as u64)
            .limit(limit as i64);

        let mut records = Vec::new();
        match session {
            Some(session) => {
                let mut cursor = find_request.session(&mut *session).await?;
                while let Some(record) = cursor.next(&mut *session).await {
                    records.push(record?);
                }
            },
            None => {
                let mut cursor = find_request.await?;
                while let Some(record) = cursor.next().await {
                    records.push(record?);
                }
            },
        }

        Ok(records)
    }

    async fn count(&self, filter: Document, limit: i64, skip: i64, session: Option<&mut ClientSession>) -> Result<u64, Error> {
        let count_request = self
            .get_collection()
            .await
            .count_documents(filter)
            .skip(skip as u64)
            .limit(limit as u64);

        let count = match session {
            Some(session) => count_request.session(&mut *session).await?,
            None => count_request.await?,

        };

        Ok(count)
    }

    async fn find_one(&self, filter: Document, session: Option<&mut ClientSession>) -> Result<Option<D>, Error> {
        let find_request = self.get_collection().await.find_one(filter);

        let cursor = match session {
            Some(session) => find_request.session(&mut *session).await?,
            None => find_request.await?,
        };

        Ok(cursor)
    }

    async fn find_one_and_replace(
        &self,
        filter: Document,
        replacement: D,
        session: Option<&mut ClientSession>,
    ) -> Result<Option<D>, Error> {
        let find_request = self
            .get_collection()
            .await
            .find_one_and_replace(filter, replacement);

        let cursor = match session {
            Some(session) => find_request.session(session).await?,
            None => find_request.await?,
        };

        Ok(cursor)
    }

    async fn insert_one(&self, record: D, session: Option<&mut ClientSession>) -> Result<(), Error> {
        let insert_request = self.get_collection().await.insert_one(record);

        match session {
            Some(session) => insert_request.session(&mut *session).await?,
            None => insert_request.await?,
        };

        Ok(())
    }

    async fn insert_many(&self, records: Vec<D>, session: Option<&mut ClientSession>) -> Result<(), Error> {
        let insert_request = self.get_collection().await.insert_many(records);

        match session {
            Some(session) => insert_request.session(&mut *session).await?,
            None => insert_request.await?,
        };

        Ok(())
    }

    async fn delete(&self, filter: Document, session: Option<&mut ClientSession>) -> Result<(), Error> {
        let collection = &self.get_collection().await;

        let delete_request = collection.delete_many(filter);

        match session {
            Some(session) => delete_request.session(&mut *session).await?,
            None => delete_request.await?,
        };

        Ok(())
    }

    async fn delete_one(
        &self,
        filter: Document,
        session: Option<&mut ClientSession>,
    ) -> Result<(), Error> {
        let delete_request = self.get_collection().await.delete_one(filter);

        match session {
            Some(session) => delete_request.session(session).await?,
            None => delete_request.await?,
        };

        Ok(())
    }

    async fn clear(&self, session: Option<&mut ClientSession>) -> Result<(), Error> {
        let delete_request = self.get_collection()
            .await
            .delete_many(doc! {});

        match session {
            Some(session) => delete_request.session(session).await?,
            None => delete_request.await?,
        };

        Ok(())
    }

    async fn drop_collection(&self, session: Option<&mut ClientSession>) -> Result<(), Error> {
        let drop_request = self.get_collection().await.drop();

        match session {
            Some(session) => drop_request.session(&mut *session).await?,
            None => drop_request.await?,
        };

        self.get_collection().await.drop().await?;
        Ok(())
    }

    async fn drop_collection_if_empty(&self, session: Option<&mut ClientSession>) -> Result<(), Error> {
        let collection = self.get_collection().await;

        if let Some(session) = session {
            if collection.find_one(doc! {}).await?.is_none() {
                collection.drop().await?;
            }
        } else {
            let mut session = self.get_collection().await.client().start_session().await?;
            session.start_transaction().await?;

            if collection.find_one(doc! {}).session(&mut session).await?.is_none() {
                collection.drop().session(&mut session).await?;
            }

            session.commit_transaction().await?;
        }


        Ok(())
    }
}
