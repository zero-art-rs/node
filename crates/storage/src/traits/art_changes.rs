use mongodb::bson::Document;

use crate::DataStorage;
use art::{BranchChanges, ART};
use types::ARTChangesRecord;
use zk::curve::cortado::{CortadoAffine as ARTG, Fr as ScalarField};

/// Storage for art full states
#[async_trait::async_trait]
pub trait ARTChangesStorage: Send + Sync + DataStorage {
    /// Store change, by pushing to the end.
    async fn push_change(&self, change: BranchChanges<ARTG>) -> Result<(), mongodb::error::Error>;
}
