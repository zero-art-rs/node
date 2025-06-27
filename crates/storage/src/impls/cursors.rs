use futures_util::TryStreamExt;
use log::info;
use mongodb::bson::from_document;
use mongodb::bson::oid::ObjectId;
use mongodb::error::Error;
use mongodb::{
    bson::{doc, Binary, DateTime, Document},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Cursor, Database, IndexModel,
};
use serde::Serialize;
use types::{CursorRecord, Message};
use uuid::Uuid;

use crate::{CursorStorage, DATABASE};

pub struct MongoCursorStorage {
    cursors_collection: Collection<CursorRecord>,
    chat_id: Uuid,
}

impl MongoCursorStorage {
    pub async fn new(chat_id: &Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().unwrap();

        let cursors_collection_name = format!("cursors/{}", chat_id);
        let cursors_collection = db.collection(&cursors_collection_name);
        let cursors_index_model = IndexModel::builder()
            .keys(doc! { "user_id": 1})
            .options(IndexOptions::builder().build())
            .build();
        cursors_collection.create_index(cursors_index_model).await?;

        Ok(Self {
            cursors_collection,
            chat_id: chat_id.clone(),
        })
    }
}

#[async_trait::async_trait]
impl CursorStorage for MongoCursorStorage {
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
                let cursor_record = CursorRecord {
                    user_id: String::from(user_id),
                    cursor: sequence_number,
                };
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
