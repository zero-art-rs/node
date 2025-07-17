use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct NatsConfig {
    /// Messages namespace
    pub messages_namespace: String,

    /// Messages namespace
    pub art_changes_namespace: String,
}
