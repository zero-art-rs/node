use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub struct MqConfig {
    pub mq_url: String,
    pub exchange_name: String,
    pub queue_name: String,
    pub routing_key: String,
}