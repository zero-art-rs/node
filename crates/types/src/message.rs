use mongodb::bson::Uuid;
use mongodb::{
    bson::{DateTime, doc},
    change_stream::{ChangeStream, event::ChangeStreamEvent},
};
use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::sync::mpsc;

#[derive(Debug)]
pub struct Subscription {
    pub chat_id: String,
    pub change_stream: ChangeStream<ChangeStreamEvent<Message>>,
    pub sender: mpsc::Sender<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Message {
    /// The message content as binary data
    pub content: Vec<u8>, // binary string, Vec<u8>
    /// When the message was created
    pub created_at: DateTime,
    /// Sequential number of this message in the chat
    pub sequence_number: i64,
    /// Public key of the message sender
    pub sender_public_key: String,
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Option<Uuid>,
}

impl Message {
    pub fn new(
        content: Vec<u8>,
        sequence_number: i64,
        sender_public_key: String,
        chat_id: Option<Uuid>,
    ) -> Self {
        Self {
            content,
            created_at: DateTime::now(),
            sequence_number,
            sender_public_key,
            chat_id,
        }
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let content_str = match std::str::from_utf8(self.content.as_slice()) {
            Ok(text) => text.to_string(),
            Err(_) => format!("0x{}", hex::encode(&self.content)),
        };

        write!(
            f,
            "[content: \"{}\", id: {}, time: {}, sender: {}]",
            content_str,
            self.sequence_number,
            self.created_at.try_to_rfc3339_string().unwrap(),
            self.sender_public_key
        )
    }
}
