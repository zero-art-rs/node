use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct StorageConfig {
    pub database_url: String,

    pub database_name: String,
}
