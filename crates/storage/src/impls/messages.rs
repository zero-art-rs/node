use mongodb::bson::from_document;
use mongodb::bson::oid::ObjectId;
use mongodb::{
    bson::{doc, Binary, DateTime, Document, Uuid},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Database, IndexModel,
};
use serde::Serialize;
use types::Message;

use crate::{MessageStorage, DATABASE};

pub struct MongoMessageStorage {
    collection: Collection<Document>,
    chat_id: Uuid,
}

impl MongoMessageStorage {
    pub async fn new(chat_id: Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().unwrap();
        let collection = db.collection(&format!("chat/{}", chat_id));

        let index_model = IndexModel::builder()
            .keys(doc! { "created_at": 1})
            .options(IndexOptions::builder().build())
            .build();
        collection.create_index(index_model).await?;

        let schema = doc! {
            "validator": {
                "$jsonSchema": {
                    "bsonType": "object",
                    "required": ["content", "created_at"],
                    "properties": {
                        "content": {
                            "bsonType": "string",
                            "description": "Message content"
                        },
                        "created_at": {
                            "bsonType": "date",
                            "description": "Message creation timestamp"
                        }
                    }
                }
            }
        };

        db.run_command(doc! {
            "collMod": "messages",
            "validator": schema["validator"].clone()
        })
        .await?;

        Ok(Self {
            collection,
            chat_id,
        })
    }

    fn get_collection(&self) -> Collection<Document> {
        let db = DATABASE.get().unwrap();
        db.collection(&format!("chat/{}", self.chat_id))
    }
}

#[async_trait::async_trait]
impl MessageStorage for MongoMessageStorage {
    async fn store_message(&self, message: Message) -> Result<(), mongodb::error::Error> {
        let document = mongodb::bson::to_document(&message)?;
        self.get_collection().insert_one(document).await?;
        Ok(())
    }

    async fn get_message(
        &self,
        created_at: &DateTime,
    ) -> Result<Option<Message>, mongodb::error::Error> {
        let filter = doc! { "created_at": created_at};
        let result = self.get_collection().find_one(filter).await?;
        Ok(result.map(|message| from_document::<Message>(message).unwrap()))
    }

    async fn get_message_by_id(
        &self,
        id: &ObjectId,
    ) -> Result<Option<Message>, mongodb::error::Error> {
        let result = self.get_collection().find_one(doc! { "_id": id }).await?;
        Ok(result.map(|message| from_document::<Message>(message).unwrap()))
    }

    async fn list_messages(
        &self,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Message>, mongodb::error::Error> {
        let mut cursor = self
            .get_collection()
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

    async fn delete_message(
        &self,
        created_at: &DateTime,
    ) -> Result<Option<Message>, mongodb::error::Error> {
        let filter = doc! { "created_at": created_at};

        let result = self.get_collection().find_one_and_delete(filter).await?;
        Ok(result.map(|message| from_document::<Message>(message).unwrap()))
    }

    async fn delete_message_by_id(
        &self,
        id: &ObjectId,
    ) -> Result<Option<Message>, mongodb::error::Error> {
        let result = self
            .get_collection()
            .find_one_and_delete(doc! { "_id": id })
            .await?;
        Ok(result.map(|message| from_document::<Message>(message).unwrap()))
    }
}
