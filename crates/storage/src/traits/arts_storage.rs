use crate::StorageError;
use bson::doc;
use cortado::CortadoAffine;
use mongodb::ClientSession;
use tracing::error;
use types::ARTRecord;
use uuid::Uuid;
use zrt_art::art::PublicArt;
use zrt_art::changes::branch_change::BranchChange;

/// Storage for art full states
#[async_trait::async_trait]
pub trait ARTStorage: Send + Sync {
    async fn new_group(
        &self,
        initial_art_record: ARTRecord<CortadoAffine>,
        session: &mut ClientSession,
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
    async fn get_art_in_session(
        &self,
        id: Uuid,
        session: &mut ClientSession,
    ) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>>;
    async fn get_art_in_session_in_lock(
        &self,
        id: Uuid,
        session: &mut ClientSession,
    ) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>>;

    async fn get_initial_art_in_session(
        &self,
        chat_id: Uuid,
        session: &mut ClientSession,
    ) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>>;
    async fn get_initial_art(
        &self,
        id: Uuid,
    ) -> mongodb::error::Result<Option<ARTRecord<CortadoAffine>>>;

    async fn get_current_epoch(&self, chat_id: &Uuid) -> Result<Option<u64>, mongodb::error::Error>;

    async fn get_current_epoch_in_session(
        &self,
        chat_id: &Uuid,
        session: &mut ClientSession,
    ) -> Result<Option<u64>, mongodb::error::Error>;

    async fn get_current_epoch_in_session_with_lock(
        &self,
        chat_id: &Uuid,
        session: &mut ClientSession,
    ) -> Result<Option<u64>, mongodb::error::Error>;

    async fn replace_art(
        &self,
        session: &mut ClientSession,
        chat_id: Uuid,
        new_art: ARTRecord<CortadoAffine>,
    ) -> Result<(), StorageError>;
}
