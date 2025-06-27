use mongodb::bson::Document;

use art::art::{BranchChanges, ART};
use types::ARTRecord;
use zk::curve::cortado::CortadoProjective as ARTG;

/// Storage for art full states
#[async_trait::async_trait]
pub trait ARTStorage: Send + Sync {
    async fn new_art(&self, art: ART<ARTG>) -> Result<(), mongodb::error::Error>;

    async fn delete_art(
        &self,
        filter: Document,
    ) -> Result<Vec<ARTRecord<ARTG>>, mongodb::error::Error>;

    async fn get_art(&self, sequence_number: i64)
        -> Result<ARTRecord<ARTG>, mongodb::error::Error>;
    async fn find_latest_art(&self) -> Result<ARTRecord<ARTG>, mongodb::error::Error>;

    /// Take the last iteration of art, update it with given changes and append to the end of the storage
    async fn update_art(&self, changes: BranchChanges<ARTG>) -> Result<(), mongodb::error::Error>;
}
