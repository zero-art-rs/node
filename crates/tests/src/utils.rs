use chrono::Local;
use std::{fmt, io};
use std::fmt::{Debug, Display};
use std::fs::{File, OpenOptions};
use tracing::Subscriber;
use tracing_subscriber::fmt::{MakeWriter, format::Writer, time::FormatTime, layer};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{filter, Layer, Registry};
use tracing_subscriber::util::SubscriberInitExt;
use {
    serde::Deserialize,
    serde_with::{base64::Base64, serde_as},
    std::collections::HashMap,
};
use regex;

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
pub fn init_tracing_for_test() {
    _ = tracing_subscriber::fmt()
        .with_timer(LocalTimer)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(true)
        .try_init();
}

pub fn logger_for_test(file_name: &str) -> impl Subscriber {
    let log_file = OpenOptions::new()
        // .append(true)
        .write(true)
        .truncate(true)
        .create(true)
        .open(file_name)
        .unwrap();

    Registry::default()
        // .with(
        //     tracing_subscriber::fmt::layer()
        //         .compact()
        //         .with_ansi(true)
        // )
        .with(
            tracing_subscriber::fmt::layer()
                // .json()
                .with_timer(LocalTimer)
                .with_target(true)
                // .pretty()
                .with_ansi(false)
                .with_writer(log_file)
                .with_filter(filter::EnvFilter::from_default_env())
        )
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

pub struct ArrayLessPrinter<T> {
    inner: T
}

impl<T> From<T> for ArrayLessPrinter<T> {
    fn from(inner: T) -> ArrayLessPrinter<T> {
        ArrayLessPrinter { inner }
    }
}

impl<T> Debug for ArrayLessPrinter<T>
where
    T: Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::new();
        fmt::write(&mut s, format_args!("{:?}", &self.inner))?;

        let re = regex::Regex::new(r"\[\s*(?:\d+,?\s*){10,}\]").unwrap();
        let rewritten = re.replace_all(&s, "[<large array>]").into_owned();

        write!(f, "{}", rewritten)
    }
}
