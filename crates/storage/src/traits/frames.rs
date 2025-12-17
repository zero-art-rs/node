use crate::StorageError;
use bson::doc;
use cortado::CortadoAffine;
use mongodb::change_stream::{event::ChangeStreamEvent, ChangeStream};
use mongodb::ClientSession;
use types::protos::group_operation::Operation;
use types::utils::ArtUpdate;
use types::FrameRecord;
use uuid::Uuid;
use zrt_art::changes::branch_change::BranchChange;

#[async_trait::async_trait]
pub trait FrameStorage: Send + Sync + Sized {
    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<FrameRecord>>, StorageError>;

    async fn next_sequence_number(&self, session: &mut ClientSession) -> Result<u64, StorageError>;
    async fn init_counter(&self, session: &mut ClientSession) -> Result<(), StorageError>;

    async fn store_message(
        &self,
        content: Vec<u8>,
        epoch: i64,
        sequence_number: u64,
        outbox_only: bool,
        operation: Option<Operation>,
        session: &mut ClientSession,
    ) -> Result<(), StorageError>;

    async fn store_message_in_session(
        &self,
        session: &mut ClientSession,
        content: Vec<u8>,
        epoch: i64,
    ) -> Result<(), mongodb::error::Error>;

    async fn get_existing_collection(chat_id: Uuid) -> Result<Self, mongodb::error::Error>;

    async fn drop_in_session(&self, session: &mut ClientSession) -> Result<(), StorageError>;

    fn extract_branch_change(
        messages: &FrameRecord,
    ) -> Result<Option<BranchChange<CortadoAffine>>, StorageError>;

    async fn get_epoch_changes(&self, id: Uuid, epoch: u64) -> Result<ArtUpdate, StorageError>;
    async fn get_epoch_changes_in_session(
        &self,
        id: Uuid,
        epoch: u64,
        session: &mut ClientSession,
    ) -> Result<ArtUpdate, StorageError>;
}
