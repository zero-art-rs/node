use log::info;
use mongodb::{
    bson::doc,
    options::IndexOptions,
    Collection, IndexModel,
};
use types::CursorRecord;
use uuid::Uuid;

use crate::{CursorStorage, DataStorage, DATABASE};

pub struct MongoCursorStorage {
    cursors_collection: Collection<CursorRecord>,
    _chat_id: Uuid,
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
            _chat_id: *chat_id,
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
}

#[async_trait::async_trait]
impl DataStorage for MongoCursorStorage {
    type Data = CursorRecord;

    async fn get_collection(&self) -> &'async_trait Collection<Self::Data> {
        &self.cursors_collection
    }
}
