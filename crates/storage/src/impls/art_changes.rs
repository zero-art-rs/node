use art::art::BranchChanges;
use futures_util::TryStreamExt;
use mongodb::{
    bson::{doc, Document},
    error::Error,
    options::IndexOptions,
    Collection, Cursor, IndexModel,
};
use uuid::Uuid;
use zk::curve::cortado::CortadoProjective as ARTG;

use crate::{ARTChangesStorage, DATABASE};
use types::ARTChangesRecord;

pub struct MongoARTChangesStorage {
    art_changes_collection: Collection<ARTChangesRecord<ARTG>>,
    _chat_id: Uuid,
}

impl MongoARTChangesStorage {
    pub async fn new(chat_id: &Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().unwrap();

        let art_changes_collection_name = format!("art_changes/{}", chat_id);
        let art_changes_collection = db.collection(&art_changes_collection_name);

        let art_changes_index_model = IndexModel::builder()
            .keys(doc! { "sequence_number": -1})
            .options(IndexOptions::builder().build())
            .build();
        art_changes_collection
            .create_index(art_changes_index_model)
            .await?;

        Ok(Self {
            art_changes_collection,
            _chat_id: *chat_id,
        })
    }
}

impl MongoARTChangesStorage {
    async fn get_recent_record(&self) -> Result<Cursor<ARTChangesRecord<ARTG>>, Error> {
        self.art_changes_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await
    }
}

#[async_trait::async_trait]
impl ARTChangesStorage for MongoARTChangesStorage {
    async fn store_change(&self, change: BranchChanges<ARTG>) -> Result<(), Error> {
        let sequence_number = match self.get_recent_record().await?.try_next().await? {
            Some(recent_record) => recent_record.sequence_number + 1,
            None => 0,
        };

        self.art_changes_collection
            .insert_one(ARTChangesRecord {
                sequence_number,
                change,
            })
            .await?;

        Ok(())
    }

    async fn list_changes(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<ARTChangesRecord<ARTG>>, Error> {
        let mut cursor = self
            .art_changes_collection
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

    async fn delete_changes(&self, filter: Document) -> Result<Vec<ARTChangesRecord<ARTG>>, Error> {
        let mut collection_cursor = self.art_changes_collection.find(filter.clone()).await?;
        let mut cursors = Vec::new();

        while let Some(cursor) = collection_cursor.try_next().await? {
            cursors.push(cursor);
        }

        self.art_changes_collection.delete_many(filter).await?;

        Ok(cursors)
    }
}
