use std::time::Duration;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub ttl: Duration,
}
