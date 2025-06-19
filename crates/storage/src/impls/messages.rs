use futures_util::TryStreamExt;
use log::info;
use mongodb::bson::from_document;
use mongodb::bson::oid::ObjectId;
use mongodb::{
    bson::{doc, Binary, DateTime, Document, Uuid},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Cursor, Database, IndexModel,
};
use mongodb::error::Error;
use serde::Serialize;
use types::{CursorRecord, Message};

use crate::{MessageStorage, DATABASE};

pub struct MongoMessageStorage {
    messages_collection: Collection<Message>,
    cursors_collection: Collection<CursorRecord>,
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

        let cursors_collection_name = format!("cursors/{}", chat_id);
        let cursors_collection = db.collection(&cursors_collection_name);
        let cursors_index_model = IndexModel::builder()
            .keys(doc! { "user_id": 1})
            .options(IndexOptions::builder().build())
            .build();
        cursors_collection.create_index(cursors_index_model).await?;

        Ok(Self {
            messages_collection,
            cursors_collection,
            chat_id: chat_id.clone(),
        })
    }
}

#[async_trait::async_trait]
impl MessageStorage for MongoMessageStorage {
    async fn store_message(&self, content: String) -> Result<(), mongodb::error::Error> {
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

        message_collection.insert_one(Message::new(content.into_bytes(), next_sequence_number)).await?;
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

    async fn update_user_cursor(
        &self,
        user_id: &str,
        sequence_number: i64,
    ) -> Result<Option<CursorRecord>, mongodb::error::Error> {
        let collection = &self.cursors_collection;

        let user_filter = doc! {"user_id": user_id.to_string()};

        let user_record = collection.find_one(user_filter.clone()).await?;
        match user_record {
            Some(record) => {
                if record.cursor < sequence_number {
                    collection
                        .update_one(
                            user_filter,
                            doc! { "$set": doc! {"cursor": sequence_number}},
                        )
                        .await?;
                }
                Ok(Some(record))
            }
            None => {
                let cursor_record = CursorRecord {user_id: String::from(user_id), cursor: sequence_number};
                collection.insert_one(cursor_record.clone()).await?;
                info!("Created new cursor_record: {}", cursor_record);
                Ok(None)
            }
        }
    }

    async fn list_cursors(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<CursorRecord>, mongodb::error::Error> {
        let mut cursor = self
            .cursors_collection
            .find(filter)
            .skip(skip as u64)
            .limit(limit)
            .await?;

        let mut records = Vec::new();
        while cursor.advance().await? {
            records.push(cursor.deserialize_current()?);
        }

        Ok(records)
    }

    async fn delete_cursors(&self, filter: Document) -> Result<Vec<CursorRecord>, Error> {
        let mut collection_cursor = self.cursors_collection.find(filter.clone()).await?;
        let mut cursors = Vec::new();

        while let Some(cursor) = collection_cursor.try_next().await? {
            cursors.push(cursor);
        }

        self.cursors_collection.delete_many(filter).await?;

        Ok(cursors)
    }
}
