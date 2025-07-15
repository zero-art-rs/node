use crate::DataStorage;
use art::types::BranchChanges;
use cortado::CortadoAffine as ARTGroup;
use mongodb::ClientSession;
use types::ProofRecord;

/// Storage for art states
#[async_trait::async_trait]
pub trait ARTChangesStorage: Send + Sync + DataStorage {
    async fn push_change(
        &self,
        session: &mut ClientSession,
        change: BranchChanges<ARTGroup>,
        proof_record: ProofRecord,
    ) -> Result<(), mongodb::error::Error>;
}
