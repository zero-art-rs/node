use mongodb::bson::{DateTime, Document, Uuid, doc, from_document, oid::ObjectId};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub content: Vec<u8>, // binary string, Vec<u8>
    pub created_at: DateTime,
    pub sequence_number: i64,
}

impl Message {
    pub fn new(content: Vec<u8>, sequence_number: i64) -> Self {
        Self {
            content,
            created_at: DateTime::now(),
            sequence_number,
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
            "[content: \"{}\", id: {}, time: {}]",
            content_str,
            self.sequence_number,
            self.created_at.try_to_rfc3339_string().unwrap()
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CursorRecord {
    pub user_id: String,
    pub cursor: i64,
}

impl fmt::Display for CursorRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[user_id: {}, cursor: {}]", self.user_id, self.cursor,)
    }
}
