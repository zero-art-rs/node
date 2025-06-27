use mongodb::bson::Document;

use art::art::BranchChanges;
use types::ARTChangesRecord;
use zk::curve::cortado::CortadoProjective as ARTG;

/// Storage for art full states
#[async_trait::async_trait]
pub trait ARTChangesStorage: Send + Sync {
    async fn store_change(&self, change: BranchChanges<ARTG>) -> Result<(), mongodb::error::Error>;

    async fn list_changes(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<ARTChangesRecord<ARTG>>, mongodb::error::Error>;

    async fn delete_changes(
        &self,
        filter: Document,
    ) -> Result<Vec<ARTChangesRecord<ARTG>>, mongodb::error::Error>;
}
