use ark_ec::{AffineRepr, CurveGroup};
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
use crypto::schnorr::{sign, verify};
use curve25519_dalek::Scalar;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use hyper::Response;
use rand::Rng;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::ops::Mul;
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

struct ARTTestContext {
    pub client: reqwest::Client,
    pub art: PrivateART<ARTGroup>,
    pub secrets: Vec<ARTScalarField>,
    pub rng: StdRng,
    pub chat_uuid: Uuid,
    pub gens: BulletproofGens,
    pub basis: PedersenBasis<CortadoAffine, Ed25519Affine>,
}

impl ARTTestContext {
    pub async fn new(size: u32) -> Self {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let secrets = (0..size).map(|_| ARTScalarField::rand(&mut rng)).collect();
        let (mut art, _) = PrivateART::new_art_from_secrets(&secrets, &ARTGroup::generator()).unwrap();

        // Create new_group for testing
        let chat_uuid = crate_new_chat(
            PublicART::new_art_from_secrets(&secrets, &ARTGroup::generator())
                .unwrap()
                .0,
            false,
        )
            .await.unwrap();

        Self {
            client: reqwest::Client::new(),
            art,
            secrets,
            rng,
            chat_uuid,
            gens: get_bulletproof_gens(),
            basis: get_pedersen_basis(),
        }
    }
}

#[tokio::test]
async fn test_send_message() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(100).await;

    let test_message = "zk messenger is the best";

    // Get Centrifugo auth token
    let centrifugo_token_response = context.client
        .post(format!("{}/{}", BACKEND_URL, "centrifugo/auth"))
        .json(&json!({
          "channels": [format!("personal:{}", context.chat_uuid)],
          "proof": BASE64_STANDARD.encode([1,2,3,4]),
          "public_key": BASE64_STANDARD.encode([1,2,3,4]),
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

    let response = context.client.get(&url).send().await?;

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

    // compute the proof
    let nonce = (0..10).map(|_| rand::random::<u8>()).collect::<Vec<u8>>();
    let index = NodeIndex::get_index_from_path(&context.art.node_index.get_path().unwrap()).unwrap();

    let mut msg = Vec::new();
    msg.extend_from_slice(context.chat_uuid.as_bytes());
    msg.extend(nonce.clone());

    let pk = vec![context.art.public_key_of(&context.art.secret_key)];

    let signature = sign(&vec![context.art.secret_key], &pk, &msg).unwrap();
    let verification_result = verify(&signature, &pk, &msg);
    assert!(verification_result.is_ok());

    // Send a message
    context.client
        .post(format!("{}/{}", BACKEND_URL, "v1/messenger/messages"))
        .json(&json!({
            "chatId": context.chat_uuid,
            "message": BASE64_STANDARD.encode(test_message.as_bytes().to_vec()),
            "signature": BASE64_STANDARD.encode(&signature),
            "nonce": BASE64_STANDARD.encode(&nonce),
        }))
        .send()
        .await?;

    let result = tokio::time::timeout(Duration::from_secs(5), handle).await?;

    if let Err(e) = result {
        panic!("Error: {:?}", e);
    }

    Ok(())
}

#[tokio::test]
async fn test_key_update() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(100).await;

    for _ in 0..10 {
        let key_update_response = update_key(
            &mut context
        ).await?;

        assert_eq!(key_update_response.status(), StatusCode::OK);
    }

    Ok(())
}

#[tokio::test]
async fn test_add_member() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(100).await;

    for i in 0..10 {
        let add_member_response = add_member(&mut context).await?;

        assert_eq!(add_member_response.status(), StatusCode::OK);
    }

    Ok(())
}

#[tokio::test]
async fn test_remove_member() -> eyre::Result<()> {
    let mut context =  ARTTestContext::new(100).await;

    for i in 1..11 {
        let remove_user_response = remove_member(
            &mut context,
            i
        )
            .await?;

        assert_eq!(remove_user_response.status(), StatusCode::NO_CONTENT);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_initial_art() -> eyre::Result<()> {
    let size = 100;
    let basis = get_pedersen_basis();
    let gens = get_bulletproof_gens();

    let client = reqwest::Client::new();

    let mut rng = StdRng::seed_from_u64(rand::random());
    let secrets = (0..size).map(|_| ARTScalarField::rand(&mut rng)).collect();
    let (mut art, _) = PrivateART::new_art_from_secrets(&secrets, &ARTGroup::generator())?;

    // Create new_group for testing
    let chat_uuid = crate_new_chat(
        PublicART {
            root: art.root.clone(),
            generator: ARTGroup::generator(),
        },
        false,
    )
    .await?;

    for i in 0..1 {
        let nonce = (0..10).map(|_| rand::random::<u8>()).collect::<Vec<u8>>();
        let index = NodeIndex::get_index_from_path(&art.node_index.get_path().unwrap()).unwrap();

        let mut msg = Vec::new();
        msg.extend_from_slice(chat_uuid.as_bytes());
        msg.extend(nonce.clone());
        msg.extend(index.to_le_bytes());

        let pk = vec![art.public_key_of(&art.secret_key)];

        let signature = sign(&vec![art.secret_key], &pk, &msg).unwrap();
        let verification_result = verify(&signature, &pk, &msg);
        assert!(verification_result.is_ok());

        let initial_art_response = client
            .get(format!("{}/{}", BACKEND_URL, "v1/messenger/initial-art"))
            .query(&json!({
                "chatId": chat_uuid,
                "signature": BASE64_STANDARD.encode(&signature),
                "index": index,
                "nonce": BASE64_STANDARD.encode(&nonce),
            }))
            .send()
            .await?;

        assert_eq!(initial_art_response.status(), StatusCode::OK);

        let received_art = PublicART::<ARTGroup>::deserialize(
            &BASE64_STANDARD
                .decode(initial_art_response.text().await.unwrap())
                .unwrap(),
        )?;
        assert_eq!(received_art.root, art.root);
        assert_eq!(received_art.generator, art.generator);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_art() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(8).await;
    // let art_roots = vec![context.art.root.clone()];
    let mut art_roots = Vec::new();

    // update art several times, so we can retrieve them
    for _ in 0..10 {
        let key_update_response = update_key(
            &mut context
        ).await?;
        art_roots.push(context.art.root.public_key);

        assert_eq!(key_update_response.status(), StatusCode::OK);

    }

    println!("{:#?}", art_roots);
    // Test if retrieval is correct
    for i in 1..11 {
        let initial_art_response = get_art(&mut context, Some(i)).await?;

        assert_eq!(initial_art_response.status(), StatusCode::OK);

        let received_art = PublicART::<ARTGroup>::deserialize(
            &BASE64_STANDARD
                .decode(initial_art_response.text().await.unwrap())
                .unwrap(),
        )?;

        assert_eq!(received_art.root.public_key, art_roots[(i - 1) as usize]);
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
            "signature": BASE64_STANDARD.encode([1, 2, 3, 4]),
            "nonce": BASE64_STANDARD.encode([1, 2, 3, 4]),

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

// update key in art, and send updates to the chat
async fn update_key(
    context: &mut ARTTestContext
) -> reqwest::Result<reqwest::Response> {
    let secret_key = context.art.secret_key.clone();
    let new_secret_key = ARTScalarField::rand(&mut context.rng);

    let associated_data = context.art.serialize().unwrap();

    let (_, key_update_changes) = context.art.update_key(&new_secret_key).unwrap();
    let (_, co_path, lambdas) = context.art.recompute_root_key_with_artefacts().unwrap();

    let blindings: Vec<_> = (0..co_path.len() + 1)
        .map(|_| Scalar::random(&mut thread_rng()))
        .collect();

    let proof = art_prove(
        &context.gens,
        context.basis.clone(),
        associated_data.as_slice(),
        co_path.clone(),
        lambdas.clone(),
        vec![secret_key],
        blindings,
    )
    .unwrap();
    let verification_result = art_verify(
        &context.gens,
        context.basis.clone(),
        associated_data.as_slice(),
        context.art.get_co_path_values(&key_update_changes.node_index.get_path().unwrap())
            .unwrap(),
        proof.clone(),
    )
    .is_ok();

    assert_eq!(verification_result, true);

    let key_update_changes_bytes = key_update_changes.serialze().unwrap();
    let mut proof_bytes = Vec::new();
    proof.serialize_uncompressed(&mut proof_bytes).unwrap();

    context.client
        .post(format!("{}/{}", BACKEND_URL, "v1/messenger/update-key"))
        .json(&json!({
          "branchChanges": BASE64_STANDARD.encode(key_update_changes_bytes),
          "chatId": context.chat_uuid,
          "proof": BASE64_STANDARD.encode(proof_bytes),
        }))
        .send()
        .await
}

// add node to the art, and send updates to the chat
async fn add_member(
    context: &mut ARTTestContext
) -> reqwest::Result<reqwest::Response> {
    let associated_data = context.art.serialize().unwrap();
    let secret_key = context.art.secret_key.clone();
    let new_user_secret_key = ARTScalarField::rand(&mut context.rng);
    let (_, append_user_changes) = context.art.append_node(&new_user_secret_key).unwrap();
    let (_, co_path, lambdas) = context.art
        .recompute_root_key_with_artefacts_using_secret_key(
            secret_key,
            Some(&append_user_changes.node_index),
        )
        .unwrap();

    let blindings: Vec<_> = (0..co_path.len() + 1)
        .map(|_| Scalar::random(&mut thread_rng()))
        .collect();

    let proof = art_prove(
        &context.gens,
        context.basis.clone(),
        associated_data.as_slice(),
        co_path.clone(),
        lambdas.clone(),
        vec![secret_key],
        blindings,
    )
        .unwrap();

    let verification_result = art_verify(
        &context.gens,
        context.basis.clone(),
        associated_data.as_slice(),
        context.art.get_co_path_values(&append_user_changes.node_index.get_path().unwrap())
            .unwrap(),
        proof.clone(),
    )
        .is_ok();

    assert_eq!(verification_result, true);

    let add_user_changes_bytes = append_user_changes.serialze().unwrap();
    let mut proof_bytes = Vec::new();
    proof.serialize_uncompressed(&mut proof_bytes).unwrap();

    context.client
        .post(format!("{}/{}", BACKEND_URL, "v1/messenger/add-member"))
        .json(&json!({
              "branchChanges": BASE64_STANDARD.encode(add_user_changes_bytes),
              "chatId": context.chat_uuid,
              "proof": BASE64_STANDARD.encode(proof_bytes),
            }))
        .send()
        .await
}

async fn remove_member(
    context: &mut ARTTestContext,
    member_id: usize,
) -> reqwest::Result<reqwest::Response> {
    let associated_data = context.art.serialize().unwrap();
    let secret_key = context.art.secret_key.clone();
    let user_to_remove = ARTGroup::generator().mul(&context.secrets[member_id]).into_affine();
    let temporary_secret_key = ARTScalarField::rand(&mut context.rng);
    let (_, remove_user_changes) = context.art
        .make_blank(&user_to_remove, &temporary_secret_key)
        .unwrap();
    let (_, co_path, lambdas) = context.art
        .recompute_root_key_with_artefacts_using_secret_key(
            temporary_secret_key,
            Some(&remove_user_changes.node_index),
        )
        .unwrap();

    let blindings: Vec<_> = (0..co_path.len() + 1)
        .map(|_| Scalar::random(&mut thread_rng()))
        .collect();

    let proof = art_prove(
        &context.gens,
        context.basis.clone(),
        associated_data.as_slice(),
        co_path.clone(),
        lambdas.clone(),
        vec![temporary_secret_key],
        blindings,
    )
        .unwrap();

    let verification_result = art_verify(
        &context.gens,
        context.basis.clone(),
        associated_data.as_slice(),
        context.art.get_co_path_values(&remove_user_changes.node_index.get_path().unwrap())
            .unwrap(),
        proof.clone(),
    )
        .is_ok();

    assert_eq!(verification_result, true);

    let remove_user_changes_bytes = remove_user_changes.serialze().unwrap();
    let mut proof_bytes = Vec::new();
    proof.serialize_uncompressed(&mut proof_bytes).unwrap();

    context.client
        .post(format!("{}/{}", BACKEND_URL, "v1/messenger/remove-member"))
        .json(&json!({
              "branchChanges": BASE64_STANDARD.encode(remove_user_changes_bytes),
              "chatId": context.chat_uuid,
              "proof": BASE64_STANDARD.encode(proof_bytes),
            }))
        .send()
        .await
}

async fn get_art(
    context: &mut ARTTestContext,
    sequence_number: Option<i64>,
) -> reqwest::Result<reqwest::Response> {
    let nonce = (0..10).map(|_| rand::random::<u8>()).collect::<Vec<u8>>();
    let index = NodeIndex::get_index_from_path(&context.art.node_index.get_path().unwrap()).unwrap();

    let mut msg = Vec::new();
    msg.extend_from_slice(context.chat_uuid.as_bytes());
    msg.extend(nonce.clone());

    let pk = vec![context.art.public_key_of(&context.art.secret_key)];

    let signature = sign(&vec![context.art.secret_key], &pk, &msg).unwrap();
    let verification_result = verify(&signature, &pk, &msg);
    assert!(verification_result.is_ok());

    context.client
        .get(format!("{}/{}", BACKEND_URL, "v1/messenger/art"))
        .query(&json!({
            "chatId": context.chat_uuid,
            "signature": BASE64_STANDARD.encode(&signature),
            "index": index,
            "nonce": BASE64_STANDARD.encode(&nonce),
            "sequenceNumber": sequence_number,
        }))
        .send()
        .await
}

