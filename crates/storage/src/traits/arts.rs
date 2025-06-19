use mongodb::bson::{doc, from_document, DateTime, Document, Uuid};
use types::ARTRecord;

use art::art::{BranchChanges, ART};
use zk::curve::cortado::{CortadoProjective as ARTG, Fr as ScalarField};

/// Storage for art full states
#[async_trait::async_trait]
pub trait ARTStorage: Send + Sync {
    async fn new_art(
        &self,
        creator_secret_key: ScalarField,
        number_of_users: i64,
    ) -> Result<ART<ARTG>, mongodb::error::Error>;

    async fn delete_art(
        &self,
        filter: Document,
    ) -> Result<Vec<ARTRecord<ARTG>>, mongodb::error::Error>;

    async fn list_art(
        &self,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<Vec<ARTRecord<ARTG>>, mongodb::error::Error>;

    /// Take the last iteration of art, update it and append to the end of the storage
    async fn update_art(
        &self,
        secret_key: ScalarField,
        changes: BranchChanges<ARTG>,
    ) -> Result<Option<ARTRecord<ARTG>>, mongodb::error::Error>;
}
