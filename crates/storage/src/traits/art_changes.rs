use crate::DataStorage;
use art::BranchChanges;
use mongodb::ClientSession;
use zk::curve::cortado::CortadoAffine as ARTGroup;

/// Storage for art states
#[async_trait::async_trait]
pub trait ARTChangesStorage: Send + Sync + DataStorage {
    async fn push_change(
        &self,
        session: &mut ClientSession,
        change: BranchChanges<ARTGroup>,
    ) -> Result<(), mongodb::error::Error>;
}
