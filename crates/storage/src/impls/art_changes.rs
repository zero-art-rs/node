use art::types::BranchChanges;
use cortado::CortadoAffine as ARTGroup;
use mongodb::{
    bson::doc, options::IndexOptions, ClientSession, Collection, IndexModel, SessionCursor,
};
use uuid::Uuid;

use crate::{ARTChangesStorage, DataStorage, StorageError, DATABASE};
use types::{ARTChangesOutboxRecord, ARTChangesRecord};

pub struct MongoARTChangesStorage {
    art_changes_collection: Collection<ARTChangesRecord<ARTGroup>>,
    art_changes_outbox_collection: Collection<ARTChangesOutboxRecord>,
}

impl MongoARTChangesStorage {
    #[inline]
    fn collection_name(chat_id: &Uuid) -> String {
        format!("art_changes/{chat_id}")
    }

    #[inline]
    fn outbox_collection_name() -> String {
        "art_changes_outbox".to_string()
    }

    /// Creates new MongoARTChangesStorage, and maps error to StorageError
    pub async fn new(chat_id: &Uuid) -> Result<Self, StorageError> {
        Self::get_storage(chat_id)
            .await
            .map_err(StorageError::MongoDB)
    }

    /// Creates new MongoARTChangesStorage but in case of error, returns mongodb::error::Error. Can be
    /// used for transactions.
    pub async fn get_storage(chat_id: &Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let art_changes_collection = db.collection(&Self::collection_name(chat_id));
        let art_changes_outbox_collection = db.collection(&Self::collection_name(chat_id));

        let art_changes_index_model = IndexModel::builder()
            .keys(doc! { "sequence_number": -1})
            .options(IndexOptions::builder().build())
            .build();
        art_changes_collection
            .create_index(art_changes_index_model)
            .await?;

        Ok(Self {
            art_changes_collection,
            art_changes_outbox_collection,
        })
    }

    pub async fn get_existing_collection(chat_id: &Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let art_changes_collection = db.collection(&Self::collection_name(chat_id));
        let art_changes_outbox_collection = db.collection(&Self::outbox_collection_name());

        Ok(Self {
            art_changes_collection,
            art_changes_outbox_collection,
        })
    }

    async fn get_recent_record(
        &self,
        session: &mut ClientSession,
    ) -> Result<SessionCursor<ARTChangesRecord<ARTGroup>>, mongodb::error::Error> {
        let records = self
            .art_changes_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .session(session)
            .await?;

        Ok(records)
    }
}

#[async_trait::async_trait]
impl DataStorage for MongoARTChangesStorage {
    type Data = ARTChangesRecord<ARTGroup>;

    async fn get_collection(&self) -> &Collection<Self::Data> {
        &self.art_changes_collection
    }
}

#[async_trait::async_trait]
impl ARTChangesStorage for MongoARTChangesStorage {
    async fn push_change(
        &self,
        session: &mut ClientSession,
        changes: BranchChanges<ARTGroup>,
        chat_id: Uuid,
        proof: Vec<u8>,
    ) -> Result<(), mongodb::error::Error> {
        let sequence_number = match self.get_recent_record(session).await?.next(session).await {
            Some(recent_record) => recent_record?.sequence_number + 1,
            None => 0,
        };

        self.art_changes_outbox_collection
            .insert_one(ARTChangesOutboxRecord::new(
                changes.serialze().map_err(|e| {
                    mongodb::error::Error::from(std::io::Error::other(e.to_string()))
                })?,
                sequence_number,
                chat_id,
                proof.clone(),
            ))
            .session(&mut *session)
            .await?;

        self.art_changes_collection
            .insert_one(ARTChangesRecord::<ARTGroup>::new(
                changes,
                sequence_number,
                chat_id,
                proof,
            ))
            .session(&mut *session)
            .await?;

        Ok(())
    }
}
