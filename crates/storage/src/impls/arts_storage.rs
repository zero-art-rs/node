use crate::StorageError;
use crate::{ARTStorage, DATABASE};
use cortado::CortadoAffine;
use mongodb::{bson::doc, options::IndexOptions, ClientSession, Collection, IndexModel};
use tracing::{debug, error, trace, warn};
use types::ARTRecord;
use uuid::Uuid;
use zrt_art::art::PublicArt;
use zrt_art::art_node::{ArtNode, TreeMethods};
use zrt_art::changes::branch_change::BranchChange;
use zrt_art::changes::ApplicableChange;
use zrt_art::errors::ArtError;
use zrt_art::node_index::NodeIndex;

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
    async fn new_group(
        &self,
        initial_art_record: ARTRecord<CortadoAffine>,
        session: &mut ClientSession,
    ) -> Result<(), mongodb::error::Error> {
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

        self.arts_collection
            .delete_one(filter.clone())
            .session(session)
            .await
            .inspect_err(|_| error!("Failed to delete art in group: {chat_id}"))?;

        Ok(())
    }

    async fn delete_initial_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id };

        self.initial_arts_collection
            .delete_one(filter)
            .session(session)
            .await
            .inspect_err(|_| error!("Failed to delete initial art in group: {chat_id}"))?;

        Ok(())
    }

    /// return the latest art
    async fn get_art(&self, id: Uuid) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>> {
        self.arts_collection.find_one(doc! {"chat_id": id}).await
    }

    async fn get_art_in_session(
        &self,
        id: Uuid,
        session: &mut ClientSession,
    ) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>> {
        self.arts_collection
            .find_one(doc! {"chat_id": id})
            .session(session)
            .await
    }

    /// Return the first art state in the chat
    async fn get_initial_art_in_session(
        &self,
        id: Uuid,
        session: &mut ClientSession,
    ) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>> {
        self.initial_arts_collection
            .find_one(doc! {"chat_id": id})
            .session(session)
            .await
    }
    
    async fn get_initial_art(
        &self,
        id: Uuid,
    ) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>> {
        self.initial_arts_collection
            .find_one(doc! {"chat_id": id})
            .await
    }

    async fn get_current_epoch(&self, chat_id: &Uuid) -> Result<Option<u64>, mongodb::error::Error> {
        let epoch = self
            .arts_collection
            .find_one(doc! { "chat_id": chat_id })
            .await?
            .map(|record| record.epoch);


        Ok(epoch)
    }

    async fn get_current_epoch_in_session(
        &self,
        chat_id: &Uuid,
        session: &mut ClientSession,
    ) -> Result<Option<u64>, mongodb::error::Error> {
        // Remove and add the data, to create a write lock on collection
        self
            .arts_collection
            .find_one(doc! { "chat_id": chat_id })
            .session(&mut *session)
            .await
            .map(|cursor| cursor.map(|r|r.epoch))
    }

    async fn get_current_epoch_in_session_with_lock(
        &self,
        chat_id: &Uuid,
        session: &mut ClientSession,
    ) -> Result<Option<u64>, mongodb::error::Error> {
        self
            .arts_collection
            .find_one_and_update(
                doc! { "chat_id": chat_id },
                doc! { "$set": { "chat_id": chat_id }}
            )
            .session(&mut *session)
            .await
            .map(|cursor| cursor.map(|r|r.epoch))
    }

    async fn replace_art(
        &self,
        session: &mut ClientSession,
        id: Uuid,
        new_art_record: ARTRecord<CortadoAffine>,
    ) -> Result<(), StorageError> {
        let _ = self
            .arts_collection
            .find_one_and_replace(doc! {"chat_id": id}, new_art_record)
            .session(session)
            .await?;

        Ok(())
    }
}
