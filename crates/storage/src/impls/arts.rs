use crate::StorageError;
use crate::{ARTStorage, DataStorage, DATABASE};
use art::{BranchChanges, ART};
use mongodb::{bson::doc, options::IndexOptions, ClientSession, Collection, IndexModel};
use types::ARTRecord;
use uuid::Uuid;
use zk::curve::cortado::CortadoAffine as ARTGroup;

pub struct MongoARTStorage {
    arts_collection: Collection<ARTRecord<ARTGroup>>,
}

impl MongoARTStorage {
    /// Creates new MongoARTStorage, and maps error to StorageError
    pub async fn new() -> Result<Self, StorageError> {
        Self::get_collection().await.map_err(StorageError::MongoDB)
    }

    /// Creates new MongoARTStorage but in case of error, returns mongodb::error::Error. Can be
    /// used for transactions.
    pub async fn get_collection() -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let arts_collection_name = "chats".to_string();
        let arts_collection = db.collection(&arts_collection_name);

        let arts_index_model = IndexModel::builder()
            .keys(doc! { "chat_id": -1})
            .options(IndexOptions::builder().build())
            .build();
        arts_collection.create_index(arts_index_model).await?;

        Ok(Self { arts_collection })
    }

    pub async fn get_existing_collection() -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let arts_collection_name = "chats".to_string();
        let arts_collection = db.collection(&arts_collection_name);

        Ok(Self { arts_collection })
    }
}

#[async_trait::async_trait]
impl DataStorage for MongoARTStorage {
    type Data = ARTRecord<ARTGroup>;

    async fn get_collection(&self) -> &Collection<Self::Data> {
        &self.arts_collection
    }
}

#[async_trait::async_trait]
impl ARTStorage for MongoARTStorage {
    async fn new_chat(
        &self,
        art: ART<ARTGroup>,
        chat_id: Uuid,
        is_private: bool,
    ) -> Result<(), StorageError> {
        self.arts_collection
            .insert_one(ARTRecord {
                chat_id,
                art,
                is_private,
            })
            .await?;

        Ok(())
    }

    async fn delete_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id };

        self.arts_collection
            .delete_one(filter)
            .session(session)
            .await?;

        Ok(())
    }

    async fn get_art(&self, chat_id: Uuid) -> Result<ARTRecord<ARTGroup>, StorageError> {
        let art = self
            .arts_collection
            .find_one(doc! {"chat_id": chat_id})
            .await?;

        art.ok_or_else(|| StorageError::NotFound)
    }

    async fn update_art(
        &self,
        changes: BranchChanges<ARTGroup>,
        chat_id: Uuid,
    ) -> Result<(), StorageError> {
        let filter = doc! { "chat_id": chat_id };

        if let Some(mut art_record) = self.arts_collection.find_one(filter.clone()).await? {
            art_record.art.update_art(&changes)?;

            self.arts_collection
                .find_one_and_replace(filter, art_record)
                .await?;
        }

        Ok(())
    }

    async fn update_art_in_session(
        &self,
        session: &mut ClientSession,
        changes: BranchChanges<ARTGroup>,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id };

        if let Some(mut art_record) = self.arts_collection.find_one(filter.clone()).await? {
            art_record
                .art
                .update_art(&changes)
                .map_err(|e| mongodb::error::Error::from(std::io::Error::other(e.to_string())))?;

            self.arts_collection
                .find_one_and_replace(filter, art_record)
                .session(session)
                .await?;
        }

        Ok(())
    }
}
