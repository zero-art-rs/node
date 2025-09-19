use crate::{DataStorage, MongoFramesStorage, StorageError, DATABASE};
use art::types::BranchChanges;
use bson::doc;
use bytes::BytesMut;
use cortado::CortadoAffine;
use mongodb::change_stream::{event::ChangeStreamEvent, ChangeStream};
use serde::de::DeserializeOwned;
use tracing::debug;
use types::protos::group_operation::Operation;
use types::protos::Frame;
use types::utils::decode_branch_changes;
use types::FrameRecord;
use uuid::Uuid;

/// A storage abstraction to store **Frames** (Group events and/or messages) in a group.
///
/// # Associated Types
///
/// - [`Data`](DataStorage::Data):
///   The type of documents stored in the MongoDB collection.
///   Must implement [`Serialize`] and [`DeserializeOwned`] for BSON conversion,
///   and also be `Send + Sync` so it can be used safely in async contexts.
///
/// - [`Error`](FrameStorage::Error):
///   The error type returned by storage operations.
///
/// - [`Session`](FrameStorage::Session):
///   A database session or transaction handle, used to group operations
///   into a consistent context.
#[async_trait::async_trait]
pub trait FrameStorage:
    DataStorage<Self::Data, Self::Error, Self::Session> + Send + Sync + Sized
{
    type Data;
    type Session;
    type Error;

    async fn new(id: Uuid) -> Result<Self, Self::Error>;

    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<FrameRecord>>, Self::Error>;

    async fn next_sequence_number(
        &self,
        session: Option<&mut Self::Session>,
    ) -> Result<u64, Self::Error>;

    async fn store_message(
        &self,
        content: Vec<u8>,
        epoch: i64,
        outbox_only: bool,
        session: Option<&mut Self::Session>,
    ) -> Result<(), Self::Error>;

    async fn drop_in_session(&self, session: Option<&mut Self::Session>)
        -> Result<(), Self::Error>;
}
