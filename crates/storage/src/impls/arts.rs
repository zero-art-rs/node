use crate::{ARTStorage, DATABASE};
use ark_ec::{AffineRepr, CurveGroup, PrimeGroup};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use ark_std::{One, UniformRand, Zero};
use art::{BranchChanges, ART};
use futures_util::TryStreamExt;
use log::info;
use mongodb::error::Error;
use mongodb::{
    bson::{doc, Binary, DateTime, Document, Uuid},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Cursor, Database, IndexModel,
};
use rand::Rng;
use types::{ARTChangesRecord, ARTRecord};
use zk::curve::cortado::{CortadoAffine as ARTG, Fr as ScalarField};

pub struct MongoARTStorage {
    arts_collection: Collection<ARTRecord<ARTG>>,
}

impl MongoARTStorage {
    pub async fn new() -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().unwrap();

        let arts_collection_name = format!("chats");
        let arts_collection = db.collection(&arts_collection_name);

        let arts_index_model = IndexModel::builder()
            .keys(doc! { "chat_id": -1})
            .options(IndexOptions::builder().build())
            .build();
        arts_collection.create_index(arts_index_model).await?;

        Ok(Self { arts_collection })
    }
}

impl MongoARTStorage {
    pub async fn chat_exists(&self, chat_id: Uuid) -> Result<bool, Error> {
        let filter = doc! {"chat_id": chat_id};
        match self.arts_collection.find_one(filter).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }
}

#[async_trait::async_trait]
impl ARTStorage for MongoARTStorage {
    async fn new_chat(&self, art: ART<ARTG>, chat_id: Uuid) -> Result<(), Error> {
        self.arts_collection
            .insert_one(ARTRecord { chat_id, art })
            .await?;

        Ok(())
    }

    async fn delete_art(&self, chat_id: Uuid) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id.clone() };

        self.arts_collection.delete_one(filter).await?;

        info!("Successfully Deleted art in chat: {}", chat_id);

        Ok(())
    }

    async fn get_art(
        &self,
        chat_id: Uuid,
    ) -> Result<ARTRecord<ARTG>, mongodb::error::Error> {
        let art = self
            .arts_collection
            .find_one(doc! {"chat_id": chat_id})
            .await?;
        
        match art {
            Some(art) => Ok(art),
            None => Err(Error::custom("Chat isn't initialized yet.")),
        }
    }

    async fn update_art(
        &self,
        changes: BranchChanges<ARTG>,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id };

        if let Some(mut art_record) = self.arts_collection.find_one(filter.clone()).await? {
            art_record.art.update_art(&changes).unwrap();

            self.arts_collection
                .find_one_and_replace(filter, art_record)
                .await?;
        }

        info!("Chat is updated successfully.");

        Ok(())
    }
}
