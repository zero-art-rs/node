use crate::domains::{
    art::service::ARTService, centrifugo::service::CentrifugoService,
    messenger::service::MessengerService,
};
use proof_verifier::ProofVerifierSender;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use types::errors::{ARTServiceError, ApiError};
use uuid::Uuid;

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub art_service: Arc<ARTService>,

    pub proof_verifier_sender: ProofVerifierSender,

    pub art_is_updating: Arc<RwLock<HashSet<Uuid>>>,
    pub challenges: Arc<RwLock<HashSet<Vec<u8>>>>,
}

impl Container {
    pub async fn art_is_updating(&self, chat_id: Uuid) -> bool {
        self.art_is_updating.read().await.contains(&chat_id)
    }

    pub async fn start_updating(&self, chat_id: Uuid) -> Result<(), ApiError> {
        if self.art_is_updating(chat_id).await {
            info!("Failed to update art. It is currently changing");
            Err(ApiError::from(ARTServiceError::ArtIsChanging))
        } else {
            self.art_is_updating.write().await.insert(chat_id);
            Ok(())
        }
    }

    pub async fn stop_updating(&self, chat_id: Uuid) {
        self.art_is_updating.write().await.remove(&chat_id);
    }
}
