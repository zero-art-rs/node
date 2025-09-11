use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use std::fmt;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use uuid::Uuid;
use bson::serde_helpers::uuid_1_as_binary;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct KeyRecord {
    /// The message content as binary data
    #[schema(value_type = Option<String>, content_encoding = "base64")]
    #[serde_as(as = "Base64")]
    pub owner_public_key: Vec<u8>,

    /// Unique identifier of the group.
    #[serde(with = "uuid_1_as_binary")]
    pub chat_id: Uuid,
}

impl fmt::Display for KeyRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ owner_public_key: {}, ", BASE64_STANDARD.encode(&self.owner_public_key))?;
        write!(f, "chat_id: {} }}", BASE64_STANDARD.encode(self.chat_id))
    }
}
