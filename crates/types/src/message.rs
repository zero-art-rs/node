use mongodb::{
    bson::{doc},
    change_stream::{ChangeStream, event::ChangeStreamEvent},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use tokio::sync::mpsc;
use uuid::Uuid;

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
    pub created_at: DateTime<Utc>,
    /// Sequential number of this message in the chat
    pub sequence_number: i64,
    /// Sequential number of epoch during which the message was sent
    pub epoch: u32,
    /// Unique identifier of the chat to send the message to.
    pub chat_id: Option<Uuid>,
}

impl Message {
    pub fn new(
        content: Vec<u8>,
        sequence_number: i64,
        epoch: u32,
        chat_id: Option<Uuid>,
    ) -> Self {
        Self {
            content,
            created_at: Utc::now(),
            sequence_number,
            epoch,
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
            "[content: \"{}\", id: {}, time: {}, epoch: {}]",
            content_str,
            self.sequence_number,
            self.created_at,
            self.epoch
        )
    }
}
