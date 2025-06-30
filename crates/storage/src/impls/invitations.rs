use crate::{DataStorage, InvitationStorage, DATABASE};
use mongodb::{
    bson::{doc, Binary, DateTime, Document, Uuid},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Cursor, Database, IndexModel,
};
use types::InvitationRecord;
use zk::curve::cortado::{CortadoAffine as ARTG, CortadoProjective, Fr as ScalarField};

pub struct MongoInvitationStorage {
    collection: Collection<InvitationRecord<ARTG>>,
    chat_id: Uuid,
}

impl MongoInvitationStorage {
    pub async fn new(chat_id: &Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().unwrap();

        let collection_name = format!("invitations/{}", chat_id);
        let collection = db.collection(&collection_name);
        let index_model = IndexModel::builder()
            .keys(doc! { "receiver_public_key": 1})
            .options(IndexOptions::builder().build())
            .build();
        collection.create_index(index_model).await?;

        Ok(Self {
            collection,
            chat_id: chat_id.clone(),
        })
    }
}

#[async_trait::async_trait]
impl InvitationStorage for MongoInvitationStorage {}

#[async_trait::async_trait]
impl DataStorage for MongoInvitationStorage {
    type Data = InvitationRecord<ARTG>;

    async fn get_collection(&self) -> &'async_trait Collection<Self::Data> {
        &self.collection
    }
}
