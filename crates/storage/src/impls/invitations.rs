use crate::{DataStorage, InvitationStorage, StorageError, DATABASE};
use mongodb::{bson::doc, options::IndexOptions, Collection, IndexModel};
use types::InvitationRecord;
use uuid::Uuid;
use cortado::CortadoAffine as ARTGroup;

pub struct MongoInvitationStorage {
    collection: Collection<InvitationRecord<ARTGroup>>,
}

impl MongoInvitationStorage {
    pub async fn new(chat_id: &Uuid) -> Result<Self, StorageError> {
        let db = DATABASE
            .get()
            .ok_or_else(|| StorageError::DatabaseRetrieval)?;

        let collection_name = format!("invitations/{}", chat_id);
        let collection = db.collection(&collection_name);
        let index_model = IndexModel::builder()
            .keys(doc! { "receiver_public_key": 1})
            .options(IndexOptions::builder().build())
            .build();
        collection.create_index(index_model).await?;

        Ok(Self { collection })
    }
}

#[async_trait::async_trait]
impl InvitationStorage for MongoInvitationStorage {}

#[async_trait::async_trait]
impl DataStorage for MongoInvitationStorage {
    type Data = InvitationRecord<ARTGroup>;

    async fn get_collection(&self) -> &Collection<Self::Data> {
        &self.collection
    }
}
