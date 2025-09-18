#[cfg(feature = "integration_tests")]
use {
    serde::Deserialize,
    serde_with::{base64::Base64, serde_as},
    std::collections::HashMap,
};

#[cfg(feature = "integration_tests")]
#[derive(Debug, Deserialize)]
pub(crate) struct CentrifugoTokenResponse {
    pub(crate) token: String,
}

#[cfg(feature = "integration_tests")]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum CentrifugoEvent {
    Connect(ConnectMessage),
    ChannelMessage(CentrifugoMessage),
}

#[cfg(feature = "integration_tests")]
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct ConnectMessage {
    connect: ConnectData,
}

#[cfg(feature = "integration_tests")]
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

#[cfg(feature = "integration_tests")]
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct SubscriptionInfo {
    recoverable: bool,
    epoch: String,
    offset: u64,
    positioned: bool,
}

#[cfg(feature = "integration_tests")]
#[derive(Debug, Deserialize)]
pub(crate) struct CentrifugoMessage {
    #[serde(rename = "pub")]
    pub(crate) publication: CentrifugoPub,
}

#[cfg(feature = "integration_tests")]
#[derive(Debug, Deserialize)]
pub(crate) struct CentrifugoPub {
    pub(crate) data: MessageData,
}

#[serde_as]
#[cfg(feature = "integration_tests")]
#[derive(Debug, Deserialize)]
pub(crate) struct MessageData {
    #[serde_as(as = "Base64")]
    pub(crate) content: Vec<u8>,
}
