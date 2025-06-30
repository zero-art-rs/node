use mongodb::bson::{doc, from_document, DateTime, Document, Uuid};
use types::ARTRecord;

use art::{BranchChanges, ART};
use zk::curve::cortado::{CortadoAffine as ARTG, Fr as ScalarField};

/// Storage for art full states
#[async_trait::async_trait]
pub trait ARTStorage: Send + Sync {
    async fn new_chat(&self, art: ART<ARTG>, chat_id: Uuid) -> Result<(), mongodb::error::Error>;

    async fn delete_art(&self, chat_id: Uuid) -> Result<(), mongodb::error::Error>;

    async fn get_art(&self, chat_id: Uuid) -> Result<ARTRecord<ARTG>, mongodb::error::Error>;
    async fn update_art(
        &self,
        changes: BranchChanges<ARTG>,
        chat_id: Uuid,
    ) -> Result<(), mongodb::error::Error>;
}
