use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ApiConfig {
    /// Address to listen of incoming connections
    pub address: SocketAddr,

    /// If true, allows to merge art changes at the same epoch
    pub merge_changes: bool,
}
