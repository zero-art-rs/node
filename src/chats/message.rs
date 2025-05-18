

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: String,
    pub content: Vec<u8>,
    pub sender_id: String, // TODO: change to PubKey
    pub chat_id: u128,
    pub created_at: DateTime<Utc>,
}
