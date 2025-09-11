use crate::{DataStorage, DATABASE};
use mongodb::{bson::doc, options::IndexOptions, Collection, IndexModel};
use types::{errors::StorageError, KeyRecord};

/// Collection to store owner public key for every chat.
pub struct MongoKeysStorage {
    pub keys_collection: Collection<KeyRecord>,
}

impl MongoKeysStorage {
    pub async fn new() -> Result<Self, StorageError> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
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
impl DataStorage for MongoKeysStorage {
    type Data = KeyRecord;

    async fn get_collection(&self) -> &Collection<Self::Data> {
        &self.keys_collection
    }
}
