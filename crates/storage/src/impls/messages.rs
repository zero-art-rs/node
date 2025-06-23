use crate::{DataStorage, MessageStorage, DATABASE};
use async_trait::async_trait;
use futures_util::TryStreamExt;
use mongodb::error::Error;
use mongodb::{
    bson::{doc, Document, Uuid},
    change_stream::{event::ChangeStreamEvent, ChangeStream},
    options::IndexOptions,
    Collection, IndexModel,
};
use std::io::Read;
use types::Message;

pub struct MongoMessageStorage {
    messages_collection: Collection<Message>,
    _chat_id: Uuid,
}

impl MongoMessageStorage {
    pub async fn new(chat_id: &Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().unwrap();

        let messages_collection_name = format!("chat/{}", chat_id);
        let messages_collection = db.collection(&messages_collection_name);

        let messages_index_model = IndexModel::builder()
            .keys(doc! { "sequence_number": -1})
            .options(IndexOptions::builder().build())
            .build();
        messages_collection
            .create_index(messages_index_model)
            .await?;

        Ok(Self {
            messages_collection,
            _chat_id: chat_id.clone(),
        })
    }
}

#[async_trait::async_trait]
impl MessageStorage for MongoMessageStorage {
    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<Message>>, mongodb::error::Error> {
        let change_stream = self.messages_collection.watch().await?;

        Ok(change_stream)
    }

    async fn store_message(
        &self,
        content: Vec<u8>,
        sender: String,
    ) -> Result<(), mongodb::error::Error> {
        let message_collection = &self.messages_collection;

        let mut cursor = message_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await?;

        let mut next_sequence_number = 0;
        if let Some(result) = cursor.try_next().await? {
            next_sequence_number = result.sequence_number + 1;
        }

        message_collection
            .insert_one(Message::new(content, next_sequence_number, sender))
            .await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl DataStorage for MongoMessageStorage {
    type Data = Message;

    async fn get_collection(&self) -> &'async_trait Collection<Self::Data> {
        &self.messages_collection
    }
}
