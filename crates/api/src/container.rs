use crate::domains::{
    art::service::ARTService, centrifugo::service::CentrifugoService,
    messenger::service::MessengerService,
};
use cortado::CortadoAffine;
use proof_verifier::ProofVerifierSender;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use uuid::Uuid;

type ChallengeHashMap = HashMap<(Uuid, CortadoAffine), Vec<u8>>;

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub art_service: Arc<ARTService>,

    pub proof_verifier_sender: ProofVerifierSender,

    pub art_is_updating: Arc<RwLock<HashMap<Uuid, bool>>>,
    pub challenges: Arc<Mutex<ChallengeHashMap>>,
}
