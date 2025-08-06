use crate::DataStorage;
use art::types::BranchChanges;
use cortado::CortadoAffine as ARTGroup;
use mongodb::ClientSession;
use uuid::Uuid;

/// Storage for art states
#[async_trait::async_trait]
pub trait ARTChangesStorage: Send + Sync + DataStorage {
    async fn push_change(
        &self,
        session: &mut ClientSession,
        changes: BranchChanges<ARTGroup>,
        chat_id: Uuid,
        proof_record: Vec<u8>,
    ) -> Result<(), mongodb::error::Error>;
}
