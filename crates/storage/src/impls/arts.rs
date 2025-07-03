use crate::{ARTStorage, DATABASE};
use art::{BranchChanges, ART};
use futures_util::TryStreamExt;
use log::info;
use mongodb::{bson::doc, error::Error, options::IndexOptions, Collection, IndexModel};
use types::ARTRecord;
use uuid::Uuid;
use zk::curve::cortado::CortadoAffine as ARTG;

pub struct MongoARTStorage {
    arts_collection: Collection<ARTRecord<ARTG>>,
}

impl MongoARTStorage {
    pub async fn new() -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().unwrap();

        let arts_collection_name = "chats".to_string();
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
    async fn new_chat(&self, art: ART<ARTG>, chat_id: Uuid, is_private: bool) -> Result<(), Error> {
        self.arts_collection
            .insert_one(ARTRecord {
                chat_id,
                art,
                is_private,
            })
            .await?;

        Ok(())
    }

    async fn delete_art(&self, chat_id: Uuid) -> Result<(), mongodb::error::Error> {
        let filter = doc! { "chat_id": chat_id.clone() };

        self.arts_collection.delete_one(filter).await?;

        info!("Successfully Deleted art in chat: {}", chat_id);

        Ok(())
    }

    async fn get_art(&self, chat_id: Uuid) -> Result<ARTRecord<ARTG>, mongodb::error::Error> {
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

        info!("Art updated successfully.");

        Ok(())
    }

    async fn list_chats(&self, limit: i64, skip: i64) -> Result<Vec<Uuid>, mongodb::error::Error> {
        let mut cursor = self
            .arts_collection
            .find(doc! {})
            .skip(skip as u64)
            .limit(limit)
            .await?;

        let mut chat_ids = Vec::new();

        while let Some(art_record) = cursor.try_next().await? {
            chat_ids.push(art_record.chat_id);
        }

        Ok(chat_ids)
    }
}
