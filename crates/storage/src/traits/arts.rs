use uuid::Uuid;

use crate::{DataStorage, StorageError};
use art::{BranchChanges, ART};
use mongodb::ClientSession;
use types::ARTRecord;
use zk::curve::cortado::CortadoAffine as ARTGroup;

/// Storage for art full states
#[async_trait::async_trait]
pub trait ARTStorage: Send + Sync + DataStorage {
    async fn new_chat(
        &self,
        art: ART<ARTGroup>,
        chat_id: Uuid,
        is_private: bool,
    ) -> Result<(), StorageError>;

    async fn delete_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error>;

    async fn get_art(&self, chat_id: Uuid) -> Result<ARTRecord<ARTGroup>, StorageError>;
    async fn update_art(
        &self,
        changes: BranchChanges<ARTGroup>,
        chat_id: Uuid,
    ) -> Result<(), StorageError>;
    async fn update_art_in_session(
        &self,
        session: &mut ClientSession,
        changes: BranchChanges<ARTGroup>,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error>;
}
