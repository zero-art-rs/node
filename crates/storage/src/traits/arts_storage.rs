use uuid::Uuid;

use crate::{StorageError};
use zrt_art::types::{BranchChanges, PublicART};
use cortado::{CortadoAffine as ARTGroup};
use mongodb::ClientSession;
use types::ARTRecord;

/// Storage for art full states
#[async_trait::async_trait]
pub trait ARTStorage: Send + Sync {
    async fn new_chat(
        &self,
        session: &mut ClientSession,
        art: PublicART<ARTGroup>,
        chat_id: Uuid,
        is_private: bool,
    ) -> Result<(), mongodb::error::Error>;

    async fn delete_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error>;

    async fn delete_initial_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error>;

    async fn get_art(&self, chat_id: Uuid) -> Result<ARTRecord<ARTGroup>, StorageError>;
    async fn get_initial_art(&self, chat_id: Uuid) -> Result<ARTRecord<ARTGroup>, StorageError>;
    async fn update_art(
        &self,
        changes: BranchChanges<ARTGroup>,
        chat_id: Uuid,
    ) -> Result<(), StorageError>;

    /// Drop initial_arts_collection and/or arts_collection if empty
    async fn drop_collection_if_empty(&self) -> Result<(), mongodb::error::Error>;

    /// Get the sequence number of the art
    async fn get_current_epoch(&self, chat_id: &Uuid) -> Result<u64, mongodb::error::Error>;

    async fn update_metadata(
        &self,
        chat_id: Uuid,
        new_metadata: Vec<u8>,
        node_index: u64,
    ) -> Result<(), StorageError>;

    async fn replace_art(
        &self,
        chat_id: Uuid,
        new_art: ARTRecord<ARTGroup>,
    ) -> Result<(), StorageError>;
}
