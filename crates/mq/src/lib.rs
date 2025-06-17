mod traits;
mod impls;
mod errors;

pub use traits::{Publisher};
pub use impls::{RabbitMqPublisher, TopicBuilder};

#[derive(Debug)]
pub struct MqConfig {
    pub mq_url: String,
    pub exchange_name: String,
    pub queue_name: String,
    pub routing_key: String,
}
