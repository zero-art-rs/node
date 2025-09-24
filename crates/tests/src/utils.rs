use std::fmt;
use chrono::Local;
use tracing_subscriber::fmt::{
    format::Writer,
    time::FormatTime,
};
use {
    serde::Deserialize,
    serde_with::{base64::Base64, serde_as},
    std::collections::HashMap,
};
use types::FrameRecord;

#[derive(Debug, Deserialize)]
pub(crate) struct CentrifugoTokenResponse {
    pub(crate) token: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum CentrifugoEvent {
    Connect(ConnectMessage),
    ChannelMessage(CentrifugoMessage),
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct ConnectMessage {
    connect: ConnectData,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct ConnectData {
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
pub(crate) struct SubscriptionInfo {
    recoverable: bool,
    epoch: String,
    offset: u64,
    positioned: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CentrifugoMessage {
    #[serde(rename = "pub")]
    pub(crate) publication: CentrifugoPub,
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub(crate) struct CentrifugoPub {
    #[serde_as(as = "Base64")]
    pub(crate) data: Vec<u8>,
}

pub(crate) struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> fmt::Result {
        let now = Local::now();
        write!(w, "[{}]", now.format("%Y-%m-%d %H:%M:%S"))
    }
}

pub(crate) fn init_tracing_for_test() {
    _ = tracing_subscriber::fmt()
        .with_timer(LocalTimer)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
}