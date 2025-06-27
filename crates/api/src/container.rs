use std::sync::Arc;

use crate::domains::{
    centrifugo::service::CentrifugoService, messenger::service::MessengerService,
};

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
}
