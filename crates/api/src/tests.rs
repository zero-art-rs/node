use std::collections::HashMap;
use std::time::Duration;

use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;

const BACKEND_URL: &str = "http://localhost:8080";
const CENTRIFUGO_URL: &str = "http://localhost:8000";
const CHAT_ID: &str = "3fa85f64-5717-4562-b3fc-2c963f66afa6";

#[derive(Debug, Deserialize)]
struct CentrifugoTokenResponse {
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CentrifugoEvent {
    Connect(ConnectMessage),
    ChannelMessage(CentrifugoMessage),
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ConnectMessage {
    connect: ConnectData,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ConnectData {
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
struct SubscriptionInfo {
    recoverable: bool,
    epoch: String,
    offset: u64,
    positioned: bool,
}

#[derive(Debug, Deserialize)]
struct CentrifugoMessage {
    #[serde(rename = "pub")]
    publication: CentrifugoPub,
}

#[derive(Debug, Deserialize)]
struct CentrifugoPub {
    data: MessageData,
}

#[derive(Debug, Deserialize)]
struct MessageData {
    content: Vec<u8>,
}

#[tokio::test]
async fn test_send_message() -> eyre::Result<()> {
    let client = reqwest::Client::new();

    let test_message = "zk messenger is the best";

    // Get Centrifugo auth token
    let centrifugo_token_response = client
        .post(format!("{}/{}", BACKEND_URL, "centrifugo/auth"))
        .json(&json!({
          "channels": ["personal:".to_owned() + CHAT_ID],
          "proof": [1,2,3,4],
          "public_key": [1,2,3,4]
        }))
        .send()
        .await?;

    let centrifugo_token_response = centrifugo_token_response
        .json::<CentrifugoTokenResponse>()
        .await?;

    let url = format!(
        "{}/{}",
        CENTRIFUGO_URL,
        "connection/uni_sse?cf_connect={\"token\":\"".to_owned()
            + &centrifugo_token_response.token
            + "\"}"
    );

    let response = client.get(&url).send().await?;

    let mut stream = response
        .bytes_stream()
        .eventsource()
        .map(|event| match event {
            Ok(event) => {
                let data = event.data;
                match serde_json::from_str::<CentrifugoEvent>(&data) {
                    Ok(event) => Ok(event),
                    Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send>),
                }
            }
            Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send>),
        });

    let handle = tokio::spawn(async move {
        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(centrifugo_event) => {
                    match centrifugo_event {
                        CentrifugoEvent::Connect(_connect_msg) => {
                            // Connection established, continue waiting for messages
                        }
                        CentrifugoEvent::ChannelMessage(channel_msg) => {
                            let content_string =
                                String::from_utf8_lossy(&channel_msg.publication.data.content);

                            assert_eq!(content_string, test_message);
                            break;
                        }
                    }
                }
                Err(_e) => {
                    continue;
                }
            }
        }
    });

    // Send a message
    client
        .post(format!("{}/{}", BACKEND_URL, "v1/messenger/messages"))
        .json(&json!({
            "chatId": CHAT_ID,
            "message": test_message,
            "senderPublicKey": "string",
        }))
        .send()
        .await?;

    let result = tokio::time::timeout(Duration::from_secs(5), handle).await?;

    if let Err(e) = result {
        panic!("Error: {:?}", e);
    }

    Ok(())
}
