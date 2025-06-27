use std::time::Duration;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct CentrifugoConfig {
    pub hmac_secret: String,
    pub ttl: Duration,
}
