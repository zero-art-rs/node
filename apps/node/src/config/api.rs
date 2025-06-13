use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ApiConfig {
    /// Address to listen of incoming connections
    pub address: SocketAddr,
}
