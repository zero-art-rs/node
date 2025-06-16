use async_trait::async_trait;
use lapin::{ Error as LapinError };
use traits::Publisher;
use errors::MqError;

pub use impls::{RabbitMqPublisher, TopicBuilder};

mod traits;
mod impls;
mod errors;

#[derive(Debug)]
pub struct MqConfig {
    pub mq_url: String,
    pub exchange_name: String,
    pub queue_name: String,
    pub routing_key: String,
}