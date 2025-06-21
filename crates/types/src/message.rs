use mongodb::bson::{DateTime, Document, Uuid, doc, from_document, oid::ObjectId};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub content: Vec<u8>, // binary string, Vec<u8>
    pub created_at: DateTime,
    pub sequence_number: i64,
    pub sender_public_key: String,
}

impl Message {
    pub fn new(content: Vec<u8>, sequence_number: i64, sender_public_key: String) -> Self {
        Self {
            content,
            created_at: DateTime::now(),
            sequence_number,
            sender_public_key,
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
