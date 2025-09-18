use uuid::Uuid;

use crate::traits::session_support::SessionSupport;
use crate::{DataStorage, MongoARTStorage, MongoDataStorage, MongoFramesStorage};
use art::types::PublicART;
use cortado::CortadoAffine;
use mongodb::{ClientSession, Collection};
use serde::de::DeserializeOwned;
use types::{ARTRecord, FrameRecord};

/// Trait for representing Art Storage. Is should store the full first and the latest states of
/// the ART.
///
/// # Associated Types
///
/// - [`Data`](DataStorage::Data):  
///   The type of documents stored in the MongoDB collection.  
///   Must implement [`Serialize`] and [`DeserializeOwned`] for BSON conversion,
///   and also be `Send + Sync` so it can be used safely in async contexts.
///
/// - [`Error`](crate::traits::frames_storage::FrameStorage::Error):
///   The error type returned by storage operations.
///
/// - [`Session`](crate::traits::frames_storage::FrameStorage::Session):
///   A database session or transaction handle, used to group operations
///   into a consistent context.
#[async_trait::async_trait]
pub trait ARTStorage:
    DataStorage<Self::Data, Self::Error, Self::Session> + Send + Sync + Sized
{
    type Data;
    type Session;
    type Error;

    /// Creates new instance of storage, or retrieves existing one, when it is
    /// already initialized
    async fn new() -> Result<Self, Self::Error>;

    /// initialize the storage with provided initial art
    async fn new_group(
        &self,
        session: &mut Self::Session,
        art: PublicART<CortadoAffine>,
        chat_id: Uuid,
        is_private: bool,
    ) -> Result<(), Self::Error>;

    async fn delete_group(
        &self,
        session: &mut Self::Session,
        chat_id: Uuid,
    ) -> Result<(), Self::Error>;

    /// Returns latest art
    async fn get_art(&self, chat_id: Uuid)
        -> Result<Option<ARTRecord<CortadoAffine>>, Self::Error>;

    /// Returns initial art
    async fn get_initial_art(
        &self,
        chat_id: Uuid,
    ) -> Result<Option<ARTRecord<CortadoAffine>>, Self::Error>;

    // /// Drop initial_arts_collection and/or arts_collection if empty
    // async fn drop_collection_if_empty(&self) -> Result<(), Self::Error>;

    /// Get the sequence number of the art
    async fn get_current_epoch(&self, chat_id: &Uuid) -> Result<u64, Self::Error>;

    async fn replace_art(
        &self,
        chat_id: Uuid,
        new_art: ARTRecord<CortadoAffine>,
    ) -> Result<Option<ARTRecord<CortadoAffine>>, Self::Error>;
}
