use crate::{KeyStorage, MongoFramesStorage, MongoSessionSupport, DATABASE};
use mongodb::{bson::doc, options::IndexOptions, ClientSession, Collection, IndexModel};
use mongodb::error::Error;
use types::{KeyRecord};
use crate::impls::data_storage::MongoDataStorage;

/// Collection to store owner public key for every chat.
pub struct MongoKeysStorage {
    pub keys_collection: Collection<KeyRecord>,
}

#[async_trait::async_trait]
impl KeyStorage for MongoKeysStorage {
    type Data = KeyRecord;
    type Session = ClientSession;
    type Error = Error;

    async fn new() -> Result<Self, Self::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            Self::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let keys_collection = db.collection(&"keys");

        let index_model = IndexModel::builder()
            .keys(doc! { "chat_id": -1})
            .options(IndexOptions::builder().build())
            .build();

        keys_collection.create_index(index_model.clone()).await?;

        Ok(Self { keys_collection })
    }
}

#[async_trait::async_trait]
impl MongoDataStorage<KeyRecord> for MongoKeysStorage {
    async fn get_collection(&self) -> &Collection<KeyRecord> {
        &self.keys_collection
    }
}

#[async_trait::async_trait]
impl MongoSessionSupport<Error, ClientSession> for MongoKeysStorage {
    async fn start_session(&self) -> Result<ClientSession, Error> {
        self.keys_collection.client().start_session().await
    }
}
