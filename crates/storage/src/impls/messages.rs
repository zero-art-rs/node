use mongodb::{
    bson::{doc, Document},
    options::{ClientOptions, IndexOptions},
    Client, Collection, IndexModel,
};
use serde::Serialize;

use crate::{MessageStorage, MongoConfig};

pub struct MongoMessageStorage {
    collection: Collection<Document>,
}

impl MongoMessageStorage {
    pub async fn new(config: MongoConfig) -> Result<Self, mongodb::error::Error> {
        let client_options = ClientOptions::parse(config.uri).await?;
        let client = Client::with_options(client_options)?;
        let db = client.database(&config.database_name);
        let collection = db.collection("messages");

        let index_model = IndexModel::builder()
            .keys(doc! { "created_at": 1 })
            .options(IndexOptions::builder().build())
            .build();
        collection.create_index(index_model).await?;

        let schema = doc! {
            "validator": {
                "$jsonSchema": {
                    "bsonType": "object",
                    "required": ["content", "created_at", "sender_id"],
                    "properties": {
                        "content": {
                            "bsonType": "string",
                            "description": "Message content"
                        },
                        "created_at": {
                            "bsonType": "date",
                            "description": "Message creation timestamp"
                        },
                        "sender_id": {
                            "bsonType": "string",
                            "description": "ID of the message sender"
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

        Ok(Self { collection })
    }
}

#[async_trait::async_trait]
impl MessageStorage for MongoMessageStorage {
    async fn store_message<T>(&self, message: T) -> Result<(), mongodb::error::Error>
    where
        T: Serialize + Send,
    {
        let doc = mongodb::bson::to_document(&message)?;
        self.collection.insert_one(doc).await?;
        Ok(())
    }

    async fn get_message(&self, id: &str) -> Result<Option<Document>, mongodb::error::Error> {
        let result = self.collection.find_one(doc! { "_id": id }).await?;
        Ok(result)
    }

    async fn list_messages(
        &self,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<Document>, mongodb::error::Error> {
        let mut cursor = self
            .collection
            .find(doc! {})
            .skip(skip as u64)
            .limit(limit)
            .await?;

        let mut messages = Vec::new();
        while cursor.advance().await? {
            messages.push(cursor.deserialize_current()?);
        }
        Ok(messages)
    }

    async fn delete_message(&self, id: &str) -> Result<(), mongodb::error::Error> {
        self.collection.delete_one(doc! { "_id": id }).await?;
        Ok(())
    }
}
