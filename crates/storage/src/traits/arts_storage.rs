use crate::StorageError;
use cortado::CortadoAffine;
use mongodb::ClientSession;
use types::ARTRecord;
use uuid::Uuid;
use zrt_art::art::PublicArt;
use zrt_art::changes::branch_change::BranchChange;

/// Storage for art full states
#[async_trait::async_trait]
pub trait ARTStorage: Send + Sync {
    async fn new_group(
        &self,
        session: &mut ClientSession,
        initial_art_record: ARTRecord<CortadoAffine>,
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

    async fn get_art(
        &self,
        chat_id: Uuid,
    ) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>>;

    async fn get_initial_art(
        &self,
        chat_id: Uuid,
    ) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>>;

    /// Get the sequence number of the art
    async fn get_current_epoch(&self, chat_id: &Uuid) -> Result<u64, mongodb::error::Error>;

    async fn replace_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
        new_art: ARTRecord<CortadoAffine>,
    ) -> Result<(), StorageError>;
}
