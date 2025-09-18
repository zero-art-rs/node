use crate::StorageError;
use crate::{
    ARTStorage, MongoDataStorage, MongoFramesStorage, MongoSessionSupport, SessionSupport, DATABASE,
};
use art::types::{BranchChangesType, NodeIndex, PublicART};
use cortado::CortadoAffine;
use mongodb::options::ClientOptions;
use mongodb::{
    bson::doc, error::Error, options::IndexOptions, ClientSession, Collection, IndexModel,
};
use tracing::{debug, error, warn};
use types::{ARTRecord, FrameRecord};
use uuid::Uuid;

pub const ARTS_COLLECTION_NAME: &str = "arts";
pub const INITIAL_ARTS_COLLECTION_NAME: &str = "initial_arts";

pub struct MongoARTStorage {
    /// Collection for the initial art state for every chat.
    pub initial_arts_collection: Collection<ARTRecord<CortadoAffine>>,
    /// Collection for the current state of the art for the chat.
    pub arts_collection: Collection<ARTRecord<CortadoAffine>>,
}

#[async_trait::async_trait]
impl ARTStorage for MongoARTStorage {
    type Data = ARTRecord<CortadoAffine>;
    type Session = ClientSession;
    type Error = Error;

    async fn new() -> Result<Self, Error> {
        let db = DATABASE
            .get()
            .ok_or_else(|| Error::from(std::io::Error::other("DATABASE is not initialized")))?;

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

    async fn new_group(
        &self,
        session: &mut ClientSession,
        art: PublicART<CortadoAffine>,
        chat_id: Uuid,
        is_private: bool,
    ) -> Result<(), Error> {
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

    async fn delete_group(&self, session: &mut ClientSession, chat_id: Uuid) -> Result<(), Error> {
        let filter = doc! { "chat_id": chat_id };

        debug!("Deleting art for chat: {chat_id}");
        self.arts_collection
            .delete_one(filter.clone())
            .session(&mut *session)
            .await?;

        debug!("Deleting initial art for chat: {chat_id}");
        self.initial_arts_collection
            .delete_one(filter)
            .session(&mut *session)
            .await?;

        Ok(())
    }

    /// Return the latest art.
    async fn get_art(&self, chat_id: Uuid) -> Result<Option<ARTRecord<CortadoAffine>>, Error> {
        debug!("Retrieving latest art for chat: {chat_id}");
        self.arts_collection
            .find_one(doc! {"chat_id": chat_id})
            .await
    }

    /// Return the first art state in the chat.
    async fn get_initial_art(
        &self,
        chat_id: Uuid,
    ) -> Result<Option<ARTRecord<CortadoAffine>>, Error> {
        debug!("Retrieving initial art for chat: {chat_id}");
        self.initial_arts_collection
            .find_one(doc! {"chat_id": chat_id})
            .await
    }

    // async fn drop_collection_if_empty(&self) -> Result<(), Error> {
    //     if self.arts_collection.find_one(doc! {}).await?.is_none() {
    //         self.arts_collection.drop().await?;
    //     }
    //
    //     if self
    //         .initial_arts_collection
    //         .find_one(doc! {})
    //         .await?
    //         .is_none()
    //     {
    //         self.initial_arts_collection.drop().await?;
    //     }
    //
    //     Ok(())
    // }

    async fn get_current_epoch(&self, chat_id: &Uuid) -> Result<u64, Error> {
        let cursor = self
            .arts_collection
            .find_one(doc! { "chat_id": chat_id })
            .await?;

        let epoch = cursor
            .ok_or_else(|| Error::from(std::io::Error::other("No records Found")))?
            .epoch;

        Ok(epoch)
    }

    async fn replace_art(
        &self,
        id: Uuid,
        new_art_record: ARTRecord<CortadoAffine>,
    ) -> Result<Option<ARTRecord<CortadoAffine>>, Error> {
        debug!("Retrieving latest art for chat: {}", id);
        self.arts_collection
            .find_one_and_replace(doc! {"chat_id": id}, new_art_record)
            .await
    }
}

// #[async_trait::async_trait]
// impl SessionSupport<Error, ClientSession> for MongoARTStorage {
//     async fn start_session(&self) -> Result<ClientSession, Error> {
//         self.arts_collection.client().start_session().await
//     }
//
//     async fn start_transaction(session: &mut ClientSession) -> Result<(), Error> {
//         session.start_transaction().await
//     }
//
//     async fn commit_transaction(session: &mut ClientSession) -> Result<(), Error> {
//         session.commit_transaction().await
//     }
// }

#[async_trait::async_trait]
impl MongoDataStorage<ARTRecord<CortadoAffine>> for MongoARTStorage {
    async fn get_collection(&self) -> &Collection<ARTRecord<CortadoAffine>> {
        &self.arts_collection
    }
}

#[async_trait::async_trait]
impl MongoSessionSupport<Error, ClientSession> for MongoARTStorage {
    async fn start_session(&self) -> Result<ClientSession, Error> {
        self.arts_collection.client().start_session().await
    }
}
