use crate::{StorageError};
use zrt_art::types::BranchChanges;
use cortado::CortadoAffine;
use mongodb::change_stream::{event::ChangeStreamEvent, ChangeStream};
use mongodb::ClientSession;
use types::FrameRecord;
use uuid::Uuid;

#[async_trait::async_trait]
pub trait FrameStorage: Send + Sync + Sized {
    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<FrameRecord>>, StorageError>;
    async fn next_sequence_number(&self) -> Result<u64, StorageError>;
    async fn store_message(
        &self,
        content: Vec<u8>,
        epoch: i64,
        outbox_only: bool,
    ) -> Result<(), StorageError>;
    async fn store_message_in_session(
        &self,
        session: &mut ClientSession,
        content: Vec<u8>,
        epoch: i64,
    ) -> Result<(), mongodb::error::Error>;
    async fn get_existing_collection(chat_id: Uuid) -> Result<Self, mongodb::error::Error>;
    async fn drop_in_session(&self, session: &mut ClientSession) -> Result<(), StorageError>;
    fn extract_branch_changes(
        messages: &FrameRecord,
    ) -> Result<Option<BranchChanges<CortadoAffine>>, StorageError>;
    async fn get_epoch_changes(
        &self,
        id: Uuid,
        epoch: u64,
    ) -> Result<Vec<BranchChanges<CortadoAffine>>, StorageError>;
}
