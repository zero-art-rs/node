use config::Config;
use serde::Deserialize;

use std::path::PathBuf;

mod api;
pub use api::ApiConfig;

mod storage;
pub use storage::StorageConfig;

mod mq;
pub use mq::MqConfig;

mod logger;
pub use logger::LoggerConfig;

#[derive(Deserialize)]
pub struct NodeConfig {
    pub api: ApiConfig,
    pub storage: StorageConfig,
    pub mq: MqConfig,

    #[serde(default)]
    pub logger: LoggerConfig,
}

impl NodeConfig {
    pub fn from_path(path: PathBuf) -> eyre::Result<Self> {
        let config = Config::builder()
            .add_source(config::File::from(path))
            .build()?;

        Ok(config.try_deserialize()?)
    }
}
