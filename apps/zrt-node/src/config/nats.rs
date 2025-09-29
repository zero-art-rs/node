use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct NatsConfig {
    /// Messages namespace
    pub messages_namespace: String,
}
