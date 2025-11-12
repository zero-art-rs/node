use chrono::Local;
use std::fmt;
use tracing_subscriber::fmt::{format::Writer, time::FormatTime};
use {
    serde::Deserialize,
    serde_with::{base64::Base64, serde_as},
    std::collections::HashMap,
};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CentrifugoTokenResponse {
    pub token: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
#[serde(untagged)]
pub enum CentrifugoEvent {
    Connect(ConnectMessage),
    ChannelMessage(CentrifugoMessage),
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ConnectMessage {
    connect: ConnectData,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ConnectData {
    client: String,
    version: String,
    subs: HashMap<String, SubscriptionInfo>,
    expires: bool,
    ttl: u64,
    ping: u64,
    session: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SubscriptionInfo {
    recoverable: bool,
    epoch: String,
    offset: u64,
    positioned: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CentrifugoMessage {
    #[serde(rename = "pub")]
    pub publication: CentrifugoPub,
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct CentrifugoPub {
    #[serde_as(as = "Base64")]
    pub data: Vec<u8>,
}

#[allow(dead_code)]
pub struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> fmt::Result {
        let now = Local::now();
        write!(w, "[{}]", now.format("%Y-%m-%d %H:%M:%S"))
    }
}

/// Create a new `tracing_subscriber` to listen tests logs, and print them in the console.
#[allow(dead_code)]
pub fn init_tracing_for_test() {
    _ = tracing_subscriber::fmt()
        .with_timer(LocalTimer)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(true)
        .try_init();
}

pub fn stringify_option<T>(any: Option<&T>) -> String
where
    T: ToString,
{
    if let Some(t) = any {
        t.to_string().to_string()[0..30].to_string()
    } else {
        "None".to_string()
    }
}
