use serde::Serialize;
use deadpool_lapin::{
    Config, Pool, Runtime,
    lapin::{
        options::{BasicPublishOptions, ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions},
        types::FieldTable,
        BasicProperties, ExchangeKind,
    }
};
use uuid::Uuid;

use crate::{Publisher, errors::MqError, TopicBuilder, MqConfig};

pub struct RabbitMqPublisher {
    pool: Pool,
    exchange_name: String,
    routing_key: String,
}

impl RabbitMqPublisher {
    pub async fn new(mq_config: MqConfig) -> Result<Self, MqError> {
        let mut cfg = Config::default();
        cfg.url = Some(mq_config.mq_url.clone());
        let pool = cfg.create_pool(Some(Runtime::Tokio1))?;

        let channel = pool.get().await?.create_channel().await?;
        channel.exchange_declare(
            mq_config.exchange_name.as_str(),
            ExchangeKind::Topic,
            ExchangeDeclareOptions{
                durable: true,
                auto_delete: false,
                internal: false,
                ..Default::default()
            },
            FieldTable::default(),
        ).await?;

        channel.queue_declare(
            mq_config.queue_name.as_str(),
            QueueDeclareOptions{
                durable: true,
                exclusive: false,
                auto_delete: false,
                ..Default::default()
            },
            FieldTable::default(),
        ).await?;

        channel.queue_bind(
            mq_config.queue_name.as_str(),
            mq_config.exchange_name.as_str(),
            TopicBuilder::new()
                .prefix(mq_config.routing_key.clone())
                .prefix(String::from("*"))
                .build().as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        ).await?;

        Ok(RabbitMqPublisher {
            pool,
            exchange_name: mq_config.exchange_name.clone(),
            routing_key: mq_config.routing_key.clone(),
        })
    }
}

#[async_trait::async_trait]
impl Publisher for RabbitMqPublisher {
    async fn send_message<T: Serialize + Send>(
        &self, chat_id: Uuid, message: T
    ) -> Result<(), MqError> {
        let connection = self.pool.get().await?;
        let channel = connection.create_channel().await?;

        let topic = TopicBuilder::new()
            .prefix(self.routing_key.clone())
            .id(chat_id)
            .build();

        let message_bytes = serde_json::to_vec(&message)?;

        channel.basic_publish(
            self.exchange_name.as_str(),
            &topic,
            BasicPublishOptions::default(),
            &message_bytes,
            BasicProperties::default()
        ).await?;

        Ok(())
    }
}