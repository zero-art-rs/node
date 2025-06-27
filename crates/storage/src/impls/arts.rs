use ark_ec::{CurveGroup, PrimeGroup};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use ark_std::{One, UniformRand, Zero};
use art::art::{BranchChanges, ART};
use futures_util::TryStreamExt;
use log::info;
use mongodb::error::Error;
use mongodb::{
    bson::{doc, Document},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Cursor, Database, IndexModel,
};
use rand::Rng;
use uuid::Uuid;
use zk::curve::cortado::{CortadoProjective as ARTG, CortadoProjective, Fr as ScalarField};

use crate::{ARTStorage, DATABASE};
use art::art_user_agent::ARTUserAgent;
use types::{ARTChangesRecord, ARTRecord};

pub struct MongoARTStorage {
    arts_collection: Collection<ARTRecord<ARTG>>,
    chat_id: Uuid,
}

impl MongoARTStorage {
    pub async fn new(chat_id: &Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().unwrap();

        let arts_collection_name = format!("arts/{}", chat_id);
        let arts_collection = db.collection(&arts_collection_name);

        let arts_index_model = IndexModel::builder()
            .keys(doc! { "sequence_number": -1})
            .options(IndexOptions::builder().build())
            .build();
        arts_collection.create_index(arts_index_model).await?;

        Ok(Self {
            arts_collection,
            chat_id: chat_id.clone(),
        })
    }
}

impl MongoARTStorage {
    pub async fn get_recent_art_record(
        &self,
    ) -> Result<Cursor<ARTRecord<CortadoProjective>>, Error> {
        let cursor = self
            .arts_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await?;

        Ok(cursor)
    }
}

#[async_trait::async_trait]
impl ARTStorage for MongoARTStorage {
    async fn new_art(&self, art: ART<ARTG>) -> Result<(), Error> {
        self.arts_collection.delete_many(doc! {}).await?;

        self.arts_collection
            .insert_one(ARTRecord {
                sequence_number: 0,
                art: art.clone(),
            })
            .await?;

        Ok(())
    }

    async fn delete_art(
        &self,
        filter: Document,
    ) -> Result<Vec<ARTRecord<ARTG>>, mongodb::error::Error> {
        let mut collection_cursor = self.arts_collection.find(filter.clone()).await?;
        let mut art_records = Vec::new();

        while let Some(art_record) = collection_cursor.try_next().await? {
            art_records.push(art_record);
        }

        self.arts_collection.delete_many(filter).await?;

        Ok(art_records)
    }

    async fn get_art(
        &self,
        sequence_number: i64,
    ) -> Result<ARTRecord<ARTG>, mongodb::error::Error> {
        let mut cursor = self
            .arts_collection
            .find(doc! {"sequence_number": sequence_number})
            .await?;

        if let Some(result) = cursor.try_next().await? {
            return Ok(result);
        }

        info!("There is no instance of arts collection for given sequence_number");
        Err(Error::custom(
            "There is no instance of arts collection for given sequence_number",
        ))
    }

    async fn find_latest_art(&self) -> Result<ARTRecord<ARTG>, mongodb::error::Error> {
        let message_collection = &self.arts_collection;

        let mut cursor = message_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await?;

        if let Some(result) = cursor.try_next().await? {
            return Ok(result);
        }

        info!("The chat isn't initialized yet");
        Err(Error::custom("The chat isn't initialized yet"))
    }

    async fn update_art(&self, changes: BranchChanges<ARTG>) -> Result<(), mongodb::error::Error> {
        let mut recent_record = self.get_recent_art_record().await?;

        if let Some(mut recent_record) = recent_record.try_next().await? {
            recent_record.art.update_branch(&changes).unwrap();

            recent_record.sequence_number += 1;
            self.arts_collection.insert_one(recent_record).await?;
        }
        Ok(())
    }
}
