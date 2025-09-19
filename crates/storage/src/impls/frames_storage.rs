use crate::impls::data_storage::MongoDataStorage;
use crate::{
    DataStorage, FrameStorage, MongoARTStorage, MongoSessionSupport, StorageError, DATABASE,
};
use art::types::BranchChanges;
use bytes::{BufMut, BytesMut};
use cortado::CortadoAffine;
use futures_util::{StreamExt, TryStreamExt};
use mongodb::{
    bson::doc,
    change_stream::{event::ChangeStreamEvent, ChangeStream},
    error::Error,
    options::IndexOptions,
    ClientSession, Collection, IndexModel,
};
use prost::Message;
use tracing::{debug, error, warn};
use types::protos::group_operation::Operation;
use types::protos::Frame;
use types::utils::decode_branch_changes;
use types::FrameRecord;
use uuid::Uuid;

pub const GROUP_COLLECTION_NAME: &str = "group";
pub const OUTBOX_COLLECTION_NAME: &str = "messages_outbox";

pub struct MongoFramesStorage {
    pub messages_collection: Collection<FrameRecord>,
    pub messages_outbox_collection: Collection<FrameRecord>,
    pub id: Uuid,
}

#[async_trait::async_trait]
impl FrameStorage for MongoFramesStorage {
    type Data = FrameRecord;
    type Session = ClientSession;
    type Error = Error;

    async fn new(id: Uuid) -> Result<Self, Error> {
        let db = DATABASE.get().ok_or_else(|| {
            Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let messages_collection_name = format!("{GROUP_COLLECTION_NAME}/{id}");
        let messages_collection = db.collection(&messages_collection_name);

        let messages_outbox_collection = db.collection(OUTBOX_COLLECTION_NAME);

        let messages_index_model = IndexModel::builder()
            .keys(doc! { "sequence_number": -1})
            .options(IndexOptions::builder().build())
            .build();
        messages_collection
            .create_index(messages_index_model)
            .await?;

        Ok(Self {
            messages_collection,
            messages_outbox_collection,
            id,
        })
    }

    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<FrameRecord>>, Error> {
        let change_stream = self.messages_collection.watch().await?;

        Ok(change_stream)
    }

    async fn next_sequence_number(
        &self,
        session: Option<&mut Self::Session>,
    ) -> Result<u64, Self::Error> {
        let message_collection = &self.messages_collection;

        let pipeline = vec![doc! { "$group": {
            "_id": null,
            "max_sequence_number": { "$max": "$sequence_number" }
        }}];

        let mut cursor = message_collection.aggregate(pipeline);
        let search_result = match session {
            Some(session) => {
                cursor
                    .session(&mut *session)
                    .await?
                    .next(&mut *session)
                    .await
            }
            None => cursor.await?.next().await,
        };

        if let Some(result) = search_result {
            let doc = result?;

            return if let Some(max_value) = doc.get_i64("max_sequence_number").ok() {
                if max_value.is_negative() {
                    error!("Max sequence number is negative");
                    return Err(Self::Error::from(std::io::Error::other(
                        "Max sequence number is negative",
                    )));
                }

                Ok(max_value as u64)
            } else {
                error!("There is no sequence number in the query");
                Err(Self::Error::from(std::io::Error::other(
                    "There is no sequence number in the query",
                )))
            };
        }

        Ok(0)
    }

    async fn store_message(
        &self,
        content: Vec<u8>,
        epoch: i64,
        outbox_only: bool,
        session: Option<&mut Self::Session>,
    ) -> Result<(), Self::Error> {
        let message_collection = &self.messages_collection;

        // let inner_session;
        // let next_sequence_number;
        // let use_inner_session;

        let (inner_session, next_sequence_number, use_inner_session) = match session {
            Some(session) => {
                let seq = self.next_sequence_number(Some(&mut *session)).await?;
                (&mut *session, seq, false)
            }
            None => {
                let seq = self.next_sequence_number(None).await?;
                (&mut self.start_session().await?, seq, false)
            }
        };

        let mut message = FrameRecord::new(content, next_sequence_number, None, epoch);

        if use_inner_session {
            inner_session.start_transaction().await?;
        }

        if !outbox_only {
            message_collection
                .insert_one(message.clone())
                .session(&mut *inner_session)
                .await?;
        }

        // change message for outbox_collection
        message.chat_id = Some(self.id);

        self.messages_outbox_collection
            .insert_one(message)
            .session(&mut *inner_session)
            .await?;

        if use_inner_session {
            inner_session.commit_transaction().await?;
        }

        Ok(())
    }

    async fn drop_in_session(
        &self,
        session: Option<&mut Self::Session>,
    ) -> Result<(), Self::Error> {
        let drop_request = self.messages_collection.drop();
        match session {
            Some(session) => drop_request.session(&mut *session).await?,
            None => drop_request.await?,
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl MongoDataStorage<FrameRecord> for MongoFramesStorage {
    async fn get_collection(&self) -> &Collection<FrameRecord> {
        &self.messages_collection
    }
}

#[async_trait::async_trait]
impl MongoSessionSupport<Error, ClientSession> for MongoFramesStorage {
    async fn start_session(&self) -> Result<ClientSession, Error> {
        self.messages_collection.client().start_session().await
    }
}
