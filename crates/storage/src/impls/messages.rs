use crate::{DataStorage, MessageStorage, StorageError, DATABASE};
use futures_util::TryStreamExt;
use mongodb::{
    bson::doc,
    change_stream::{event::ChangeStreamEvent, ChangeStream},
    options::IndexOptions,
    ClientSession, Collection, IndexModel,
};
use tracing::debug;
use types::MessageRecord;
use uuid::Uuid;

pub struct MongoMessageStorage {
    messages_collection: Collection<MessageRecord>,
    messages_outbox_collection: Collection<MessageRecord>,
    chat_id: Uuid,
}

impl MongoMessageStorage {
    pub async fn new(chat_id: &Uuid) -> Result<Self, StorageError> {
        let db = DATABASE
            .get()
            .ok_or_else(|| StorageError::DatabaseRetrieval)?;

        let messages_collection_name = format!("chat/{chat_id}");
        let messages_collection = db.collection(&messages_collection_name);

        let messages_outbox_collection_name = "messages_outbox";
        let messages_outbox_collection = db.collection(messages_outbox_collection_name);

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
            chat_id: *chat_id,
        })
    }
}

#[async_trait::async_trait]
impl MessageStorage for MongoMessageStorage {
    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<MessageRecord>>, StorageError> {
        let change_stream = self.messages_collection.watch().await?;

        Ok(change_stream)
    }

    async fn store_message(&self, content: Vec<u8>, epoch: i64) -> Result<(), StorageError> {
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

        let mut message = MessageRecord::new(content, next_sequence_number, None, epoch);
        let mut session = self.messages_collection.client().start_session().await?;
        session.start_transaction().await?;

        message_collection
            .insert_one(message.clone())
            .session(&mut session)
            .await?;

        // change message for outbox_collection
        message.chat_id = Some(self.chat_id);

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
    ) -> Result<(), mongodb::error::Error> {
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

        let mut message = MessageRecord::new(content, next_sequence_number, None, epoch);

        message_collection
            .insert_one(message.clone())
            .session(&mut *session)
            .await?;

        // change message for outbox_collection
        message.chat_id = Some(self.chat_id);

        self.messages_outbox_collection
            .insert_one(message)
            .session(&mut *session)
            .await?;

        Ok(())
    }

    async fn get_existing_collection(chat_id: Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let messages_collection = db.collection(&format!("chat/{}", &chat_id));
        let messages_outbox_collection = db.collection(&"messages_outbox");

        Ok(Self {
            messages_collection,
            messages_outbox_collection,
            chat_id,
        })
    }
}

#[async_trait::async_trait]
impl DataStorage for MongoMessageStorage {
    type Data = MessageRecord;

    async fn get_collection(&self) -> &Collection<Self::Data> {
        &self.messages_collection
    }
}
