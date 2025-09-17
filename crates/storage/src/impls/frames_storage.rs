use crate::{DataStorage, FrameStorage, MongoARTStorage, MongoSessionSupport, StorageError, DATABASE};
use art::types::BranchChanges;
use bytes::{BufMut, BytesMut};
use cortado::CortadoAffine;
use futures_util::TryStreamExt;
use mongodb::{
    error::Error,
    bson::doc,
    change_stream::{event::ChangeStreamEvent, ChangeStream},
    options::IndexOptions,
    ClientSession, Collection, IndexModel,
};
use prost::Message;
use tracing::debug;
use types::protos::group_operation::Operation;
use types::protos::Frame;
use types::utils::decode_branch_changes;
use types::FrameRecord;
use uuid::Uuid;
use crate::impls::data_storage::MongoDataStorage;

pub const GROUP_COLLECTION_NAME: &str = "group";
pub const OUTBOX_COLLECTION_NAME: &str = "messages_outbox";

pub struct MongoFramesStorage {
    pub messages_collection: Collection<FrameRecord>,
    pub messages_outbox_collection: Collection<FrameRecord>,
    pub id: Uuid,
}

#[async_trait::async_trait]
impl FrameStorage for MongoFramesStorage {
    type Data = FrameRecord;
    type Session = ClientSession;
    type Error = Error;

    async fn new(id: Uuid) -> Result<Self, Self::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            Self::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let messages_collection_name = format!("{GROUP_COLLECTION_NAME}/{id}");
        let messages_collection = db.collection(&messages_collection_name);

        let messages_outbox_collection = db.collection(OUTBOX_COLLECTION_NAME);

        let messages_index_model = IndexModel::builder()
            .keys(doc! { "sequence_number": -1})
            .options(IndexOptions::builder().build())
            .build();
        messages_collection
            .create_index(messages_index_model)
            .await?;

        Ok(Self {
            messages_collection,
            messages_outbox_collection,
            id,
        })
    }

    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<FrameRecord>>, Self::Error> {
        let change_stream = self.messages_collection.watch().await?;

        Ok(change_stream)
    }

    async fn next_sequence_number(&self) -> Result<u64, Self::Error> {
        let message_collection = &self.messages_collection;

        let mut cursor = message_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await?;

        let next_sequence_number = match cursor.try_next().await? {
            Some(result) => result.sequence_number + 1,
            None => 0,
        };

        Ok(next_sequence_number)
    }

    async fn store_message(
        &self,
        content: Vec<u8>,
        epoch: i64,
        outbox_only: bool,
    ) -> Result<(), Self::Error> {
        let message_collection = &self.messages_collection;

        let next_sequence_number = self.next_sequence_number().await?;

        let mut message = FrameRecord::new(content, next_sequence_number, None, epoch);
        let mut session = self.messages_collection.client().start_session().await?;
        session.start_transaction().await?;

        if !outbox_only {
            message_collection
                .insert_one(message.clone())
                .session(&mut session)
                .await?;
        }

        // change message for outbox_collection
        message.chat_id = Some(self.id);

        self.messages_outbox_collection
            .insert_one(message)
            .session(&mut session)
            .await?;

        session.commit_transaction().await?;

        Ok(())
    }

    async fn store_message_in_session(
        &self,
        session: &mut ClientSession,
        content: Vec<u8>,
        epoch: i64,
    ) -> Result<(), Self::Error> {
        let message_collection = &self.messages_collection;

        let mut cursor = message_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await?;

        let next_sequence_number = match cursor.try_next().await? {
            Some(result) => result.sequence_number + 1,
            None => 0,
        };
        debug!(
            "Store message with sequence number {}",
            next_sequence_number
        );

        let mut message = FrameRecord::new(content, next_sequence_number, None, epoch);

        message_collection
            .insert_one(message.clone())
            .session(&mut *session)
            .await?;

        // change message for outbox_collection
        message.chat_id = Some(self.id);

        self.messages_outbox_collection
            .insert_one(message)
            .session(&mut *session)
            .await?;

        Ok(())
    }

    async fn get_existing_collection(chat_id: Uuid) -> Result<Self, Self::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let messages_collection = db.collection(&format!("{GROUP_COLLECTION_NAME}/{}", &chat_id));
        let messages_outbox_collection = db.collection(OUTBOX_COLLECTION_NAME);

        Ok(Self {
            messages_collection,
            messages_outbox_collection,
            id: chat_id,
        })
    }

    async fn drop_in_session(&self, session: &mut ClientSession) -> Result<(), Self::Error> {
        self.messages_collection
            .drop()
            .session(&mut *session)
            .await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl MongoDataStorage<FrameRecord> for MongoFramesStorage {
    async fn get_collection(&self) -> &Collection<FrameRecord> {
        &self.messages_collection
    }
}

#[async_trait::async_trait]
impl MongoSessionSupport<Error, ClientSession> for MongoFramesStorage {
    async fn start_session(&self) -> Result<ClientSession, Error> {
        self.messages_collection.client().start_session().await
    }
}
