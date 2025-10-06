use crate::StorageError;
use crate::{ARTStorage, DATABASE};
use cortado::CortadoAffine;
use mongodb::{bson::doc, options::IndexOptions, ClientSession, Collection, IndexModel};
use tracing::{debug, error, warn};
use types::ARTRecord;
use uuid::Uuid;
use zrt_art::types::NodeIndex;
use zrt_art::{
    traits::ARTPublicAPI,
    types::{BranchChanges, PublicART},
};

pub const ARTS_COLLECTION_NAME: &str = "arts";
pub const INITIAL_ARTS_COLLECTION_NAME: &str = "initial_arts";

pub struct MongoARTStorage {
    /// Collection for the initial art state for every chat.
    pub initial_arts_collection: Collection<ARTRecord<CortadoAffine>>,
    /// Collection for the current state of the art for the chat.
    pub arts_collection: Collection<ARTRecord<CortadoAffine>>,
}

impl MongoARTStorage {
    /// Creates new MongoARTStorage, and maps error to StorageError
    pub async fn new() -> Result<Self, StorageError> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let arts_collection = db.collection(ARTS_COLLECTION_NAME);
        let initial_arts_collection = db.collection(INITIAL_ARTS_COLLECTION_NAME);

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

        let arts_collection = db.collection(ARTS_COLLECTION_NAME);
        let initial_arts_collection = db.collection(INITIAL_ARTS_COLLECTION_NAME);

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
        session: &mut ClientSession,
        art: PublicART<CortadoAffine>,
        chat_id: Uuid,
        is_private: bool,
    ) -> Result<(), mongodb::error::Error> {
        let initial_art_record = ARTRecord {
            chat_id,
            art: art.clone(),
            is_private,
            epoch: 0,
        };

        self.arts_collection
            .insert_one(initial_art_record.clone())
            .session(&mut *session)
            .await?;

        self.initial_arts_collection
            .insert_one(initial_art_record)
            .session(&mut *session)
            .await?;

        Ok(())
    }

    async fn delete_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id };

        debug!("Deleting art for chat: {chat_id}");
        self.arts_collection
            .delete_one(filter.clone())
            .session(session)
            .await?;

        Ok(())
    }

    async fn delete_initial_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id };

        debug!("Deleting initial art for chat: {chat_id}");
        self.initial_arts_collection
            .delete_one(filter)
            .session(session)
            .await?;

        Ok(())
    }

    /// return the latest art
    async fn get_art(&self, chat_id: Uuid) -> Result<ARTRecord<CortadoAffine>, StorageError> {
        debug!("Retrieving latest art for chat: {chat_id}");
        let art = self
            .arts_collection
            .find_one(doc! {"chat_id": chat_id})
            .await?;

        art.ok_or_else(|| StorageError::NotFound)
    }

    /// Return the first art state in the chat
    async fn get_initial_art(
        &self,
        chat_id: Uuid,
    ) -> Result<ARTRecord<CortadoAffine>, StorageError> {
        debug!("Retrieving initial art for chat: {chat_id}");
        let art = self
            .initial_arts_collection
            .find_one(doc! {"chat_id": chat_id})
            .await?;

        art.ok_or_else(|| StorageError::NotFound)
    }

    async fn update_art(
        &self,
        changes: BranchChanges<CortadoAffine>,
        chat_id: Uuid,
    ) -> Result<(), StorageError> {
        let filter = doc! { "chat_id": chat_id };

        debug!("Updating art for chat: {}", chat_id);
        if let Some(mut art_record) = self.arts_collection.find_one(filter.clone()).await? {
            art_record.art.update_public_art(&changes)?;

            self.arts_collection
                .find_one_and_replace(filter, art_record)
                .await?;

            debug!("Art updated successfully");
        } else {
            warn!("Art not found");
            return Err(StorageError::NotFound);
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
        }

        Ok(())
    }

    async fn get_current_epoch(&self, chat_id: &Uuid) -> Result<u64, mongodb::error::Error> {
        let cursor = self
            .arts_collection
            .find_one(doc! { "chat_id": chat_id })
            .await?;

        let epoch = cursor
            .ok_or_else(|| mongodb::error::Error::from(std::io::Error::other("No records Found")))?
            .epoch;

        Ok(epoch)
    }

    async fn update_metadata(
        &self,
        chat_id: Uuid,
        new_metadata: Vec<u8>,
        node_index: u64,
    ) -> Result<(), StorageError> {
        let mut art = self.get_art(chat_id).await?;
        art.art
            .get_mut_node(&NodeIndex::Index(node_index))?
            .metadata = Some(new_metadata);
        self.replace_art(chat_id, art).await?;

        Ok(())
    }

    async fn replace_art(
        &self,
        id: Uuid,
        new_art_record: ARTRecord<CortadoAffine>,
    ) -> Result<(), StorageError> {
        debug!("Retrieving latest art for chat: {}", id);
        let art = self
            .arts_collection
            .find_one_and_replace(doc! {"chat_id": id}, new_art_record)
            .await?;

        if art.is_none() {
            error!("No art found for chat: {id}");
            return Err(StorageError::NotFound);
        }

        Ok(())
    }
}
