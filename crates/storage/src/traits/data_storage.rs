use crate::StorageError;
use bson::doc;
use futures_util::TryStreamExt;
use mongodb::{bson::Document, ClientSession, Collection};
use mongodb::error::Error;
use serde::{de::DeserializeOwned, Serialize};
use tracing::debug;

/// It is a generic abstraction storage of typed data. It requires some default methods, which
/// are the same for different storages.
///
/// # Type Parameters
/// * `D` - The type of documents stored in the MongoDB collection.  
/// * `E` - The type of documents stored in the MongoDB collection.
/// * `S` — The type representing a session handle.
#[async_trait::async_trait]
pub trait DataStorage<D, E, S>: Send + Sync {
    async fn list(
        &self,
        filter: Document,
        sort_option: Option<Document>,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<D>, E>;

    async fn count(&self, filter: Document, limit: i64, skip: i64) -> Result<u64, E>;

    async fn find_one(&self, filter: Document) -> Result<Option<D>, E>;

    async fn find_one_and_replace(&self, filter: Document, replacement: D, session: Option<&mut S>) -> Result<Option<D>, E>;

    async fn insert_one(&self, record: D) -> Result<(), E>;

    async fn insert_many(&self, records: Vec<D>) -> Result<(), E>;

    async fn delete(&self, filter: Document) -> Result<Vec<D>, E>;

    async fn delete_one(&self, filter: Document, session: Option<&mut S>) -> Result<(), Error>;

    async fn clear(&self, session: &mut S) -> Result<(), E>;

    async fn drop_collection(&self) -> Result<(), E>;

    async fn drop_collection_in_session(
        &self,
        sesion: &mut S,
    ) -> Result<(), E>;

    async fn drop_collection_if_empty(&self) -> Result<(), E>;
}

/// It is a generic abstraction for MongoDB-backed storage of typed data. It
/// contains a default implementation of general methods.   
///
/// # Type Parameters
/// * `D` - The type of documents stored in the MongoDB collection.  
///
/// # Usage
///
/// One can implement this trait for a specific Mongo-backed storage struct
/// that owns a `Collection<T>`.  
///
/// ```ignore
/// #[async_trait::async_trait]
/// impl MongoDataStorage<ARTRecord<CortadoAffine>> for MongoARTStorage {
///     async fn get_collection(&self) -> &Collection<ARTRecord<CortadoAffine>> {
///         &self.arts_collection
///     }
/// }
/// ```
#[async_trait::async_trait]
pub trait MongoDataStorage<D>: Send + Sync
where
    D: Serialize + DeserializeOwned + Send + Sync,
{
    async fn get_collection(&self) -> &Collection<D>;
}
