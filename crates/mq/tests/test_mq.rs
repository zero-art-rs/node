#[cfg(test)]
mod tests {
    use uuid::Uuid;
    use mq::{Publisher, MqConfig, RabbitMqPublisher, TopicBuilder};

    /// Tests whether the TopicBuilder creates topic strings as intended
    #[test]
    fn test_topic_builder() {
        let topic_builder = TopicBuilder::new();
        let topic = topic_builder
            .prefix(String::from("chats"))
            .prefix(String::from("personal"))
            .id(Uuid::max())
            .build();

        assert_eq!(
            *topic,
            String::from(format!("chats.personal.{}", Uuid::max()))
        );
    }

    /// Tests send_message and includes setting up Publisher
    // TODO: divide into two distinct tests for Publisher setup and message sending
    #[tokio::test]
    async fn test_publisher_send_message() {
        let opts = MqConfig {
            mq_url: String::from("amqp://user:password@localhost:5672"),
            exchange_name: String::from("messages_exchange"),
            queue_name: String::from("messages_queue"),
            routing_key: String::from("chats"),
        };

        let publisher = RabbitMqPublisher::new(opts).await.unwrap();
        let message = String::from("hello there");

        assert_eq!(publisher.send_message(
            Uuid::max(),
            message.clone(),
        ).await.unwrap(), ())
    }
}
