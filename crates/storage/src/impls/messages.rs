use crate::{MessageStorage, DATABASE};
use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, Binary, DateTime, Document, Uuid},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Cursor, Database, IndexModel,
};
use types::Message;

pub struct MongoMessageStorage {
    messages_collection: Collection<Message>,
    chat_id: Uuid,
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
            chat_id: chat_id.clone(),
        })
    }
}

#[async_trait::async_trait]
impl MessageStorage for MongoMessageStorage {
    async fn store_message(
        &self,
        content: String,
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
            .insert_one(Message::new(
                content.into_bytes(),
                next_sequence_number,
                sender,
            ))
            .await?;
        Ok(())
    }

    async fn list_messages(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Message>, mongodb::error::Error> {
        let mut cursor = self
            .messages_collection
            .find(filter)
            .skip(skip as u64)
            .limit(limit)
            .await?;

        let mut messages = Vec::new();
        while cursor.advance().await? {
            messages.push(cursor.deserialize_current()?);
        }

        Ok(messages)
    }

    async fn delete_messages(
        &self,
        filter: Document,
    ) -> Result<Vec<Message>, mongodb::error::Error> {
        let mut collection_cursor = self.messages_collection.find(filter.clone()).await?;
        let mut messages = Vec::new();

        while let Some(message) = collection_cursor.try_next().await? {
            messages.push(message);
        }

        self.messages_collection.delete_many(filter).await?;

        Ok(messages)
    }
}
