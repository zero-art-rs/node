use crate::impls::frames::CounterRecord;
use crate::{DataStorage, DATABASE};
use mongodb::{bson::doc, options::IndexOptions, ClientSession, Collection, Database, IndexModel};
use tracing::debug;
use types::{errors::StorageError, KeyRecord};
use uuid::Uuid;

const KEYS_COLLECTION_NAME: &str = "keys";

/// Collection to store owner public key for every chat.
pub struct MongoKeysStorage {
    keys_collection: Collection<KeyRecord>,
}

impl MongoKeysStorage {
    pub async fn init_key_collection(db: &Database) -> Result<(), StorageError> {
        let keys_collection = db.collection::<KeyRecord>(KEYS_COLLECTION_NAME);

        let index_model = IndexModel::builder()
            .keys(doc! { "chat_id": -1})
            .options(IndexOptions::builder().build())
            .build();

        keys_collection.create_index(index_model.clone()).await?;

        Ok(())
    }

    pub async fn new() -> Result<Self, StorageError> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let keys_collection = db.collection(KEYS_COLLECTION_NAME);

        Ok(Self { keys_collection })
    }

    pub async fn init_group(
        &self,
        owner_id_pub_key: Vec<u8>,
        id: Uuid,
        session: &mut ClientSession,
    ) -> Result<(), StorageError> {
        let existing = self
            .keys_collection
            .find_one(doc! { "chat_id": id })
            .session(&mut *session)
            .await?;

        if existing.is_some() {
            return Err(StorageError::RecordAlreadyExists);
        }

        self.keys_collection
            .insert_one(KeyRecord::new(owner_id_pub_key, id))
            .session(&mut *session)
            .await?;

        Ok(())
    }

    pub async fn delete_group(
        &self,
        id: Uuid,
        session: &mut ClientSession,
    ) -> Result<(), StorageError> {
        self.keys_collection
            .delete_one(doc! {"chat_id": id})
            .session(session)
            .await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl DataStorage for MongoKeysStorage {
    type Data = KeyRecord;

    async fn get_collection(&self) -> &Collection<Self::Data> {
        &self.keys_collection
    }
}
