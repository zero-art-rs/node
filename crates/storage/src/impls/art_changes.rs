use art::BranchChanges;
use futures_util::TryStreamExt;
use mongodb::{bson::doc, error::Error, options::IndexOptions, Collection, Cursor, IndexModel};
use uuid::Uuid;
use zk::curve::cortado::CortadoAffine as ARTG;

use crate::{ARTChangesStorage, DataStorage, DATABASE};
use types::ARTChangesRecord;

pub struct MongoARTChangesStorage {
    art_changes_collection: Collection<ARTChangesRecord<ARTG>>,
    _chat_id: Uuid,
}

impl MongoARTChangesStorage {
    #[inline]
    pub fn collection_name(chat_id: &Uuid) -> String {
        format!("art_changes/{}", chat_id)
    }

    pub async fn new(chat_id: &Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().unwrap();

        let art_changes_collection = db.collection(&Self::collection_name(chat_id));

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

    async fn get_recent_record(&self) -> Result<Cursor<ARTChangesRecord<ARTG>>, Error> {
        self.art_changes_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await
    }
}

#[async_trait::async_trait]
impl DataStorage for MongoARTChangesStorage {
    type Data = ARTChangesRecord<ARTG>;

    async fn get_collection(&self) -> &'async_trait Collection<Self::Data> {
        &self.art_changes_collection
    }
}

#[async_trait::async_trait]
impl ARTChangesStorage for MongoARTChangesStorage {
    async fn push_change(&self, change: BranchChanges<ARTG>) -> Result<(), Error> {
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
}
