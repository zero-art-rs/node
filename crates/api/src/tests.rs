use ark_ec::AffineRepr;
use ark_ed25519::EdwardsAffine as Ed25519Affine;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::{
    prelude::StdRng,
    {SeedableRng, thread_rng},
};
use ark_std::{One, UniformRand, Zero};
use art::errors::ARTError;
use art::traits::{ARTPrivateAPI, ARTPublicAPI};
use art::types::{NodeIndex, PrivateART, PublicART};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use bulletproofs::{BulletproofGens, PedersenGens};
use cortado::{CortadoAffine as ARTGroup, CortadoAffine, Fr as ARTScalarField};
use curve25519_dalek::Scalar;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;
use zk::art::{art_prove, art_verify};
use zkp::toolbox::cross_dleq::PedersenBasis;
use zkp::toolbox::dalek_ark::ristretto255_to_ark;

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

#[derive(Debug, Deserialize)]
struct InitChatResponse {
    token: String,
}

async fn crate_new_chat(art: PublicART<ARTGroup>, is_private: bool) -> eyre::Result<Uuid> {
    let chat_id = Uuid::now_v7();
    let client = reqwest::Client::new();

    let delete_response = client
        .delete(format!("{}/{}", BACKEND_URL, "v1/messenger/chat"))
        .json(&json!({
          "chatId": chat_id,
        }))
        .send()
        .await;

    let init_response = client
        .post(format!("{}/{}", BACKEND_URL, "v1/messenger/init-chat"))
        .json(&json!({
          "art": BASE64_STANDARD.encode(art.serialize()?),
          "chatId": chat_id,
          "isPrivate": is_private
        }))
        .send()
        .await;

    Ok(chat_id)
}

fn get_pedersen_basis() -> PedersenBasis<CortadoAffine, Ed25519Affine> {
    let g_1 = CortadoAffine::generator();
    let h_1 = CortadoAffine::new_unchecked(cortado::ALT_GENERATOR_X, cortado::ALT_GENERATOR_Y);

    let gens = PedersenGens::default();
    PedersenBasis::<CortadoAffine, Ed25519Affine>::new(
        g_1,
        h_1,
        ristretto255_to_ark(gens.B).unwrap(),
        ristretto255_to_ark(gens.B_blinding).unwrap(),
    )
}

fn get_bulletproof_gens() -> BulletproofGens {
    BulletproofGens::new(2048, 1)
}

#[tokio::test]
async fn test_key_update() -> eyre::Result<()> {
    let size = 100;
    let basis = get_pedersen_basis();
    let gens = get_bulletproof_gens();

    let client = reqwest::Client::new();

    let mut rng = StdRng::seed_from_u64(rand::random());
    let secrets = (0..size).map(|_| ARTScalarField::rand(&mut rng)).collect();
    let (mut art, _) = PrivateART::new_art_from_secrets(&secrets, &ARTGroup::generator())?;

    // Create new_group for testing
    let chat_uuid = crate_new_chat(
        PublicART::new_art_from_secrets(&secrets, &ARTGroup::generator())
            .unwrap()
            .0,
        false,
    )
    .await?;

    // Repeat test several times
    for _ in 0..10 {
        let secret_key = art.secret_key.clone();
        let new_secret_key = ARTScalarField::rand(&mut rng);

        let associated_data = art.serialize().unwrap();

        let (_, key_update_changes) = art.update_key(&new_secret_key).unwrap();
        let (_, co_path, lambdas) = art.recompute_root_key_with_artefacts().unwrap();

        let blindings: Vec<_> = (0..co_path.len() + 1)
            .map(|_| Scalar::random(&mut thread_rng()))
            .collect();

        let proof = art_prove(
            &gens,
            basis.clone(),
            associated_data.as_slice(),
            co_path.clone(),
            lambdas.clone(),
            vec![secret_key],
            blindings,
        )
        .unwrap();
        let verification_result = art_verify(
            &gens,
            basis.clone(),
            associated_data.as_slice(),
            art.get_co_path_values(&key_update_changes.node_index.get_path().unwrap())
                .unwrap(),
            proof.clone(),
        )
        .is_ok();

        assert_eq!(verification_result, true);

        let key_update_changes_bytes = key_update_changes.serialze().unwrap();
        let mut proof_bytes = Vec::new();
        proof.serialize_uncompressed(&mut proof_bytes).unwrap();

        let key_update_response = client
            .post(format!("{}/{}", BACKEND_URL, "v1/messenger/update-key"))
            .json(&json!({
              "branchChanges": BASE64_STANDARD.encode(key_update_changes_bytes),
              "chatId": chat_uuid,
              "proof": BASE64_STANDARD.encode(proof_bytes),
            }))
            .send()
            .await?;

        assert_eq!(key_update_response.status(), StatusCode::OK);
    }

    Ok(())
}

#[tokio::test]
async fn test_add_user() -> eyre::Result<()> {
    let size = 100;
    let basis = get_pedersen_basis();
    let gens = get_bulletproof_gens();

    let client = reqwest::Client::new();

    let mut rng = StdRng::seed_from_u64(rand::random());
    let secrets = (0..size).map(|_| ARTScalarField::rand(&mut rng)).collect();
    let (mut art, _) = PrivateART::new_art_from_secrets(&secrets, &ARTGroup::generator())?;

    // Create new_group for testing
    let chat_uuid = crate_new_chat(
        PublicART::new_art_from_secrets(&secrets, &ARTGroup::generator())
            .unwrap()
            .0,
        false,
    )
    .await?;

    for i in 0..10 {
        let associated_data = art.serialize().unwrap();
        let secret_key = art.secret_key.clone();
        let new_user_secret_key = ARTScalarField::rand(&mut rng);
        let (_, append_user_changes) = art.append_node(&new_user_secret_key).unwrap();
        let (_, co_path, lambdas) = art
            .recompute_root_key_with_artefacts_using_secret_key(
                secret_key,
                Some(&append_user_changes.node_index),
            )
            .unwrap();

        let blindings: Vec<_> = (0..co_path.len() + 1)
            .map(|_| Scalar::random(&mut thread_rng()))
            .collect();

        let proof = art_prove(
            &gens,
            basis.clone(),
            associated_data.as_slice(),
            co_path.clone(),
            lambdas.clone(),
            vec![secret_key],
            blindings,
        )
        .unwrap();

        let verification_result = art_verify(
            &gens,
            basis.clone(),
            associated_data.as_slice(),
            art.get_co_path_values(&append_user_changes.node_index.get_path().unwrap())
                .unwrap(),
            proof.clone(),
        )
        .is_ok();

        assert_eq!(verification_result, true);

        let add_user_changes_bytes = append_user_changes.serialze().unwrap();
        let mut proof_bytes = Vec::new();
        proof.serialize_uncompressed(&mut proof_bytes).unwrap();

        let key_update_response = client
            .post(format!("{}/{}", BACKEND_URL, "v1/messenger/add-member"))
            .json(&json!({
              "branchChanges": BASE64_STANDARD.encode(add_user_changes_bytes),
              "chatId": chat_uuid,
              "proof": BASE64_STANDARD.encode(proof_bytes),
            }))
            .send()
            .await?;

        assert_eq!(key_update_response.status(), StatusCode::OK);
    }

    Ok(())
}
