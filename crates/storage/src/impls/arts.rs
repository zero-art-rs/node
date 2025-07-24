use crate::StorageError;
use crate::{ARTStorage, DATABASE};
use art::{
    traits::ARTPublicAPI,
    types::{BranchChanges, PublicART},
};
use cortado::CortadoAffine as ARTGroup;
use log::info;
use mongodb::{bson::doc, options::IndexOptions, ClientSession, Collection, IndexModel};
use types::ARTRecord;
use uuid::Uuid;

pub struct MongoARTStorage {
    /// Collection for the initial art state for every chat.
    pub initial_arts_collection: Collection<ARTRecord<ARTGroup>>,
    /// Collection for the current state of the art for the chat.
    pub arts_collection: Collection<ARTRecord<ARTGroup>>,
}

impl MongoARTStorage {
    /// Creates new MongoARTStorage, and maps error to StorageError
    pub async fn new() -> Result<Self, StorageError> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let arts_collection = db.collection("chats".as_ref());
        let initial_arts_collection = db.collection("initial_chats".as_ref());

        let index_model = IndexModel::builder()
            .keys(doc! { "chat_id": -1})
            .options(IndexOptions::builder().build())
            .build();

        arts_collection.create_index(index_model.clone()).await?;
        initial_arts_collection
            .create_index(index_model.clone())
            .await?;

        Ok(Self {
            arts_collection,
            initial_arts_collection,
        })
    }

    pub async fn get_existing_storage() -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let arts_collection = db.collection("chats".as_ref());
        let initial_arts_collection = db.collection("initial_chats".as_ref());

        Ok(Self {
            arts_collection,
            initial_arts_collection,
        })
    }
}

#[async_trait::async_trait]
impl ARTStorage for MongoARTStorage {
    async fn new_chat(
        &self,
        art: PublicART<ARTGroup>,
        chat_id: Uuid,
        is_private: bool,
    ) -> Result<(), StorageError> {
        let initial_art_record = ARTRecord {
            chat_id,
            art: art.clone(),
            is_private,
            sequence_number: 0,
        };

        self.arts_collection
            .insert_one(initial_art_record.clone())
            .await?;

        self.initial_arts_collection
            .insert_one(initial_art_record)
            .await?;

        Ok(())
    }

    async fn delete_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id };

        info!("Deleting art for chat: {}", chat_id);
        self.arts_collection
            .delete_one(filter.clone())
            .session(session)
            .await?;

        info!("Art deleted successfully");
        Ok(())
    }

    async fn delete_initial_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id };

        info!("Deleting initial art for chat: {}", chat_id);
        self.initial_arts_collection
            .delete_one(filter)
            .session(session)
            .await?;

        info!("Initial art deleted successfully");
        Ok(())
    }

    /// return the latest art
    async fn get_art(&self, chat_id: Uuid) -> Result<ARTRecord<ARTGroup>, StorageError> {
        info!("Retrieving latest art for chat: {}", chat_id);
        let art = self
            .arts_collection
            .find_one(doc! {"chat_id": chat_id})
            .await?;

        art.ok_or_else(|| StorageError::NotFound)
    }

    /// Return the first art state in the chat
    async fn get_initial_art(&self, chat_id: Uuid) -> Result<ARTRecord<ARTGroup>, StorageError> {
        info!("Retrieving initial art for chat: {}", chat_id);
        let art = self
            .initial_arts_collection
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

        info!("Updating art for chat: {}", chat_id);
        if let Some(mut art_record) = self.arts_collection.find_one(filter.clone()).await? {
            art_record.art.update_public_art(&changes)?;

            self.arts_collection
                .find_one_and_replace(filter, art_record)
                .await?;

            info!("Art updated successfully");
        } else {
            info!("Art not found");
            return Err(StorageError::NotFound);
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

        info!("Updating art for chat: {}", chat_id);
        if let Some(mut art_record) = self.arts_collection.find_one(filter.clone()).await? {
            art_record
                .art
                .update_public_art(&changes)
                .map_err(|e| mongodb::error::Error::from(std::io::Error::other(e.to_string())))?;
            art_record.sequence_number += 1;

            self.arts_collection
                .find_one_and_replace(filter, art_record)
                .session(session)
                .await?;
            info!("Art updated successfully");
        } else {
            info!("Art not found");
            return Err(mongodb::error::Error::from(std::io::Error::other(
                StorageError::NotFound,
            )));
        }

        Ok(())
    }

    async fn drop_collection_if_empty(&self) -> Result<(), mongodb::error::Error> {
        if self.arts_collection.find_one(doc! {}).await?.is_none() {
            self.arts_collection.drop().await?;
        }

        if self
            .initial_arts_collection
            .find_one(doc! {})
            .await?
            .is_none()
        {
            self.initial_arts_collection.drop().await?;
            info!("Collection dropped successfully");
        } else {
            info!("Failed to drop collection. It isn't empty");
        }

        Ok(())
    }

    async fn get_latest_sequence_number(
        &self,
        chat_id: &Uuid,
    ) -> Result<i64, mongodb::error::Error> {
        let cursor = self
            .arts_collection
            .find_one(doc! { "chat_id": chat_id })
            .await?;

        let sequence_number = cursor
            .ok_or_else(|| mongodb::error::Error::from(std::io::Error::other("No records Found")))?
            .sequence_number;

        Ok(sequence_number)
    }
}
