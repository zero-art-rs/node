use futures_util::TryStreamExt;
use log::info;
use mongodb::bson::from_document;
use mongodb::bson::oid::ObjectId;
use mongodb::{
    bson::{doc, Binary, DateTime, Document, Uuid},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Cursor, Database, IndexModel,
};
use serde::Serialize;
use types::{CursorRecord, Message};

use crate::{MessageStorage, DATABASE};

pub struct MongoMessageStorage {
    messages_collection: Collection<Document>,
    cursors_collection: Collection<Document>,
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

        let messages_schema = doc! {
            "validator": {
                "$jsonSchema": {
                    "bsonType": "object",
                    "required": ["content", "created_at"],
                    "properties": {
                        "content": {
                            "bsonType": "array",
                            "description": "Message content"
                        },
                        "created_at": {
                            "bsonType": "date",
                            "description": "Message creation timestamp"
                        },
                        "sequence_number": {
                            "bsonType": "long",
                            "description": "Message sequence number"
                        }
                    }
                }
            }
        };

        db.run_command(doc! {
            "collMod": messages_collection_name,
            "validator": messages_schema["validator"].clone()
        })
        .await?;

        let cursors_collection_name = format!("cursors/{}", chat_id);
        let cursors_collection = db.collection(&cursors_collection_name);
        let cursors_index_model = IndexModel::builder()
            .keys(doc! { "user_id": 1})
            .options(IndexOptions::builder().build())
            .build();
        cursors_collection.create_index(cursors_index_model).await?;

        let cursors_schema = doc! {
            "validator": {
                "$jsonSchema": {
                    "bsonType": "object",
                    "required": ["user_id", "cursor"],
                    "properties": {
                        "user_id": {
                            "bsonType": "string",
                            "description": "Users unique identifier"
                        },
                        "cursor": {
                            "bsonType": "long",
                            "description": "Sequence umber of last read message"
                        }
                    }
                }
            }
        };

        db.run_command(doc! {
            "collMod": cursors_collection_name,
            "validator": cursors_schema["validator"].clone()
        })
        .await?;

        Ok(Self {
            messages_collection,
            cursors_collection,
            chat_id: chat_id.clone(),
        })
    }

    fn get_messages_collection(&self) -> Collection<Document> {
        let db = DATABASE.get().unwrap();
        db.collection(&format!("chat/{}", self.chat_id))
    }

    fn get_cursors_collection(&self) -> Collection<Document> {
        let db = DATABASE.get().unwrap();
        db.collection(&format!("cursors/{}", self.chat_id))
    }
}

#[async_trait::async_trait]
impl MessageStorage for MongoMessageStorage {
    async fn store_message(&self, content: String) -> Result<(), mongodb::error::Error> {
        let message_collection = self.get_messages_collection();

        let mut cursor = message_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await?;

        let mut next_sequence_number = 0;
        if let Some(result) = cursor.try_next().await? {
            next_sequence_number = result.get_i64("sequence_number").unwrap() + 1;
        }

        let message = Message::new(content.into_bytes(), next_sequence_number);
        let document = mongodb::bson::to_document(&message)?;
        message_collection.insert_one(document).await?;
        Ok(())
    }

    async fn get_message(
        &self,
        created_at: &DateTime,
    ) -> Result<Option<Message>, mongodb::error::Error> {
        let message_filter = doc! { "created_at": created_at};
        let result = self
            .get_messages_collection()
            .find_one(message_filter)
            .await?;

        // let cursors_filter = doc! { "created_at": created_at};
        // let cursors = self.get_cursors_collection().find_one()

        Ok(result.map(|message| from_document::<Message>(message).unwrap()))
    }

    async fn get_message_by_id(
        &self,
        id: &ObjectId,
    ) -> Result<Option<Message>, mongodb::error::Error> {
        let result = self
            .get_messages_collection()
            .find_one(doc! { "_id": id })
            .await?;
        Ok(result.map(|message| from_document::<Message>(message).unwrap()))
    }

    async fn list_messages(
        &self,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Message>, mongodb::error::Error> {
        let mut cursor = self
            .get_messages_collection()
            .find(doc! {})
            .skip(skip as u64)
            .limit(limit)
            .await?;

        let mut messages = Vec::new();
        while cursor.advance().await? {
            messages.push(cursor.deserialize_current()?);
        }

        let result = messages
            .iter()
            .map(|message| from_document::<Message>(message.clone()).unwrap())
            .collect();

        Ok(result)
    }

    async fn list_cursors(
        &self,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<CursorRecord>, mongodb::error::Error> {
        let mut cursor = self
            .get_cursors_collection()
            .find(doc! {})
            .skip(skip as u64)
            .limit(limit)
            .await?;

        let mut records = Vec::new();
        while cursor.advance().await? {
            records.push(cursor.deserialize_current()?);
        }

        let result = records
            .iter()
            .map(|message| from_document::<CursorRecord>(message.clone()).unwrap())
            .collect();

        Ok(result)
    }

    async fn delete_message(
        &self,
        created_at: &DateTime,
    ) -> Result<Option<Message>, mongodb::error::Error> {
        let filter = doc! { "created_at": created_at};

        let result = self
            .get_messages_collection()
            .find_one_and_delete(filter)
            .await?;
        Ok(result.map(|message| from_document::<Message>(message).unwrap()))
    }

    async fn delete_message_by_id(
        &self,
        id: &ObjectId,
    ) -> Result<Option<Message>, mongodb::error::Error> {
        let result = self
            .get_messages_collection()
            .find_one_and_delete(doc! { "_id": id })
            .await?;
        Ok(result.map(|message| from_document::<Message>(message).unwrap()))
    }

    async fn update_user_cursor(
        &self,
        user_id: &str,
        sequence_number: i64,
    ) -> Result<Option<CursorRecord>, mongodb::error::Error> {
        let collection = self.get_cursors_collection();

        let user_filter = doc! {"user_id": user_id.to_string()};

        let user_record = collection.find_one(user_filter.clone()).await?;
        match user_record {
            Some(record) => {
                if record.get_i64("cursor").unwrap() < sequence_number {
                    collection
                        .update_one(
                            user_filter,
                            doc! { "$set": doc! {"cursor": sequence_number}},
                        )
                        .await?;
                }
                Ok(Some(from_document::<CursorRecord>(record)?))
            }
            None => {
                let cursor_record = doc! {"user_id": user_id, "cursor": sequence_number};
                collection.insert_one(cursor_record.clone()).await?;
                info!("Created new cursor_record: {}", cursor_record);
                Ok(None)
            }
        }
    }
}
