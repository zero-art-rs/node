use ark_ec::PrimeGroup;
use ark_std::rand::{rngs::StdRng, SeedableRng};
use ark_std::{One, UniformRand, Zero};
use art::art::{BranchChanges, ART};
use futures_util::TryStreamExt;
use mongodb::error::Error;
use mongodb::{
    bson::{doc, Binary, DateTime, Document, Uuid},
    options::{ClientOptions, IndexOptions},
    Client, Collection, Cursor, Database, IndexModel,
};
use zk::curve::cortado::{CortadoProjective as ARTG, Fr as ScalarField};

use crate::{ARTStorage, DATABASE};
use types::ARTRecord;

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

#[async_trait::async_trait]
impl ARTStorage for MongoARTStorage {
    async fn new_art(
        &self,
        creator_secret_key: ScalarField,
        number_of_users: i64,
    ) -> Result<ART<ARTG>, Error> {
        let mut secrets = vec![creator_secret_key];

        for i in 1..number_of_users {
            secrets.push(ScalarField::rand(
                &mut StdRng::seed_from_u64(rand::random()),
            ));
        }

        let (art, _) = ART::new_art_from_secrets(&secrets, &ARTG::generator());

        let record = ARTRecord {
            sequence_number: 0,
            art: art.clone(),
        };

        self.arts_collection.insert_one(record).await?;

        Ok(art)
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

    async fn list_art(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<ARTRecord<ARTG>>, mongodb::error::Error> {
        let mut cursor = self
            .arts_collection
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

    async fn update_art(
        &self,
        secret_key: ScalarField,
        changes: BranchChanges<ARTG>,
    ) -> Result<Option<ARTRecord<ARTG>>, mongodb::error::Error> {
        let mut cursor = self
            .arts_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await?;

        if let Some(mut record) = cursor.try_next().await? {
            record.art.update_branch(&changes).unwrap();
            let root_key = record.art.recompute_root_key(secret_key);

            record.sequence_number += 1;
            self.arts_collection.insert_one(record.clone()).await?;

            return Ok(Some(record));
        }
        Ok(None)
    }
}
