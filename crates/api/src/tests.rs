use ark_ec::{AffineRepr, CurveGroup};
use ark_ed25519::EdwardsAffine as Ed25519Affine;
use ark_serialize::CanonicalSerialize;
use ark_std::{
    UniformRand,
    rand::prelude::StdRng,
    rand::{SeedableRng, thread_rng},
};
use art::errors::ARTError;
use art::traits::{ARTPrivateAPI, ARTPrivateView, ARTPublicAPI, ARTPublicView};
use art::types::{NodeIndex, NodeIterWithPath, PrivateART, PublicART};
use base64::{Engine, prelude::BASE64_STANDARD};
use bulletproofs::{BulletproofGens, PedersenGens};
use cortado::{CortadoAffine as ARTGroup, CortadoAffine, Fr as ARTScalarField};
use crypto::schnorr::{sign, verify};
use curve25519_dalek::Scalar;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use hyper::Response;
use rand::{Rng, random};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use serde_with::{base64::Base64, serde_as};
use std::iter::Skip;
use std::{collections::HashMap, ops::Mul, time::Duration};
use tracing::info;
use types::{ARTChangesRecord, Message};
use types::art_schemas::*;
use types::centrifugo_schemas::AuthRequest;
use uuid::Uuid;
use zk::art::{art_prove, art_verify};
use zkp::toolbox::{cross_dleq::PedersenBasis, dalek_ark::ristretto255_to_ark};

const BACKEND_URL: &str = "http://localhost:8080";
const CENTRIFUGO_URL: &str = "http://localhost:8000";
// used for tests, which can be repeated
const TEST_REPEATS: usize = 2;
const DEFAULT_NONCE_LENGTH: u32 = 16; // 16 bytes

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

#[serde_as]
#[derive(Debug, Deserialize)]
struct MessageData {
    #[serde_as(as = "Base64")]
    content: Vec<u8>,
}

#[derive(Clone)]
struct ARTTestContext {
    pub client: reqwest::Client,
    pub art: PrivateART<ARTGroup>,
    pub initial_secrets: Vec<ARTScalarField>,
    pub rng: StdRng,
    pub chat_uuid: Uuid,
    pub basis: PedersenBasis<CortadoAffine, Ed25519Affine>,
}

impl ARTTestContext {
    pub async fn new(size: i64) -> Self {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let secrets = (0..size).map(|_| ARTScalarField::rand(&mut rng)).collect();
        let (art, _) = PrivateART::new_art_from_secrets(&secrets, &ARTGroup::generator()).unwrap();

        // Create new_group for testing
        let chat_uuid = crate_new_chat(
            PublicART::new_art_from_secrets(&secrets, &ARTGroup::generator())
                .unwrap()
                .0,
            false,
        )
        .await
        .unwrap();

        Self {
            client: reqwest::Client::new(),
            art,
            initial_secrets: secrets,
            rng,
            chat_uuid,
            basis: get_pedersen_basis(),
        }
    }

    pub fn get_index(&self) -> Result<i64, art::errors::ARTError> {
        Ok(NodeIndex::get_index_from_path(&self.art.node_index.get_path()?)? as i64)
    }

    pub fn derive_new(&self, index: i64) -> Result<Self, ARTError> {
        let (mut art, _) =
            PrivateART::new_art_from_secrets(&self.initial_secrets, &CortadoAffine::generator())?;
        art.secret_key = self.initial_secrets[index as usize].clone();
        art.update_node_index();

        Ok(Self {
            client: reqwest::Client::new(),
            art,
            initial_secrets: self.initial_secrets.clone(),
            rng: self.rng.clone(),
            chat_uuid: self.chat_uuid,
            basis: get_pedersen_basis(),
        })
    }
}

use mongodb::bson::{DateTime, doc};
use types::messenger_schemas::{GetMessageQuery, CountMessagesQuery};

#[tokio::test]
async fn test_send_message() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(100).await;

    let test_message = "zk messenger is the best";

    let challenge_response = get_challenge(&mut context).await?;
    assert_eq!(challenge_response.status(), StatusCode::OK);

    let challenge = BASE64_STANDARD
        .decode(challenge_response.text().await.unwrap())
        .unwrap();

    let nonce = (0..DEFAULT_NONCE_LENGTH)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();

    let tk = context.art.recompute_root_key().unwrap().key;
    let pk = context.art.get_root().public_key;

    let signature = sign(&vec![tk], &vec![pk], &challenge).unwrap();

    // Get Centrifugo auth token
    let centrifugo_token_response = context
        .client
        .post(format!("{}/{}", BACKEND_URL, "centrifugo/auth"))
        .json(&AuthRequest {
            chat_ids: vec![context.chat_uuid],
            proof: signature,
            nonce,
            challenge,
            epochs: vec![0],
        })
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
    let nonce = (0..DEFAULT_NONCE_LENGTH)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();
    let mut msg = Vec::new();
    msg.extend_from_slice(context.chat_uuid.as_bytes());
    msg.extend(&nonce);

    let tk = context.art.recompute_root_key()?.key;
    let pk = vec![context.art.root.public_key];

    let signature = sign(&vec![tk], &pk, &msg).unwrap();
    let verification_result = verify(&signature, &pk, &msg);
    assert!(verification_result.is_ok());

    // Send a message
    context
        .client
        .post(format!(
            "{}/{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid, "messages"
        ))
        .json(&json!({
            // "chatId": context.chat_uuid,
            "message": BASE64_STANDARD.encode(test_message.as_bytes().to_vec()),
            "signature": BASE64_STANDARD.encode(&signature),
            "nonce": BASE64_STANDARD.encode(&nonce),
            "epoch": 1i64,
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
async fn test_key_and_metadata_update() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(100).await;
    let mut test_context = context.derive_new(2)?;

    for i in 0..TEST_REPEATS {
        let metadata = Some((0..100).map(|_| rand::random::<u8>()).collect::<Vec<u8>>());
        let payload = Some((0..100).map(|_| rand::random::<u8>()).collect::<Vec<u8>>());

        let key_update_response =
            update_key(&mut context, metadata.clone(), payload.clone()).await?;
        assert_eq!(key_update_response.status(), StatusCode::OK);

        let changes_response = get_changes(&mut test_context, 1000, i as i64, 0).await?;
        assert_eq!(changes_response.status(), StatusCode::OK);

        let changes = postcard::from_bytes::<Vec<ARTChangesRecord<CortadoAffine>>>(
            &changes_response.bytes().await?,
        )?;

        assert_eq!(changes.len(), 1);

        // test_context.art.update_public_art(&changes[0].changes)?;
    }

    // use None instead of some to use old metadata
    let key_update_response = update_key(&mut context, None, None).await?;
    assert_eq!(key_update_response.status(), StatusCode::OK);

    let changes_response = get_changes(&mut test_context, 1000, TEST_REPEATS as i64, 0).await?;
    assert_eq!(changes_response.status(), StatusCode::OK);

    let changes = postcard::from_bytes::<Vec<ARTChangesRecord<CortadoAffine>>>(
        &changes_response.bytes().await?,
    )?;

    assert_eq!(changes.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_add_member() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(100).await;

    for _ in 0..TEST_REPEATS {
        let add_member_response = add_member(&mut context).await?;

        assert_eq!(add_member_response.status(), StatusCode::OK);
    }

    Ok(())
}

#[tokio::test]
async fn test_add_member_after_removal() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(100).await;

    for i in 0..TEST_REPEATS {
        let remove_member_response = make_blank(&mut context, i + 1).await?;
        let add_member_response = add_member(&mut context).await?;

        assert_eq!(add_member_response.status(), StatusCode::OK);
    }

    Ok(())
}

#[tokio::test]
async fn test_remove_member() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(100).await;
    let mut retrieval_context = context.derive_new(2)?;

    for i in 1..TEST_REPEATS + 1 {
        // skip the root node
        let remove_user_response = make_blank(&mut context, i).await?;
        assert_eq!(remove_user_response.status(), StatusCode::NO_CONTENT);

        let new_art_response = get_art(&mut retrieval_context, i as i64).await?;
        assert_eq!(new_art_response.status(), StatusCode::OK);

        let received_art = PublicART::<ARTGroup>::deserialize(
            &BASE64_STANDARD
                .decode(new_art_response.text().await.unwrap())
                .unwrap(),
        )?;
        assert_eq!(
            received_art.root.weight,
            retrieval_context.art.root.weight - 1
        );
        retrieval_context.art =
            PrivateART::from_public_art(received_art, context.art.secret_key).unwrap();
    }

    Ok(())
}

#[tokio::test]
async fn test_get_art() -> eyre::Result<()> {
    let mut context = ARTTestContext::new(100).await;
    let mut retrieval_context = context.derive_new(2)?;
    let mut art_roots = vec![context.art.root.public_key];

    // update art several times, so we can retrieve them
    for _ in 0..TEST_REPEATS {
        let key_update_response = update_key(&mut context, None, None).await?;
        art_roots.push(context.art.root.public_key);

        assert_eq!(key_update_response.status(), StatusCode::OK);
    }

    // Test if retrieval is correct
    for i in 0..TEST_REPEATS {
        let art_response = get_art(&mut retrieval_context, i as i64).await?;

        assert_eq!(art_response.status(), StatusCode::OK);

        let received_art = PublicART::<ARTGroup>::deserialize(
            &BASE64_STANDARD
                .decode(art_response.text().await.unwrap())
                .unwrap(),
        )?;

        assert_eq!(received_art.root.public_key, art_roots[(i) as usize]);
        retrieval_context.art =
            PrivateART::from_public_art(received_art, retrieval_context.initial_secrets[1])
                .unwrap();
    }

    Ok(())
}

#[tokio::test]
async fn test_delete_chat() -> eyre::Result<()> {
    let context = ARTTestContext::new(100).await;
    let nonce = (0..DEFAULT_NONCE_LENGTH)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();

    let mut msg = Vec::new();
    msg.extend_from_slice(context.chat_uuid.as_bytes());
    msg.extend(nonce.clone());

    let pk = vec![context.art.public_key_of(&context.art.secret_key)];
    let signature = sign(&vec![context.art.secret_key], &pk, &msg).unwrap();
    let verification_result = verify(&signature, &pk, &msg);
    assert!(verification_result.is_ok());

    let delete_response = context
        .client
        .delete(format!(
            "{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid
        ))
        .query(&json!({
            "chatId": context.chat_uuid,
            "signature": BASE64_STANDARD.encode(&signature),
            "nonce": BASE64_STANDARD.encode(&nonce),
        }))
        .send()
        .await?;

    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    Ok(())
}

async fn crate_new_chat(art: PublicART<ARTGroup>, is_private: bool) -> eyre::Result<Uuid> {
    let chat_id = Uuid::now_v7();
    let client = reqwest::Client::new();

    let init_response = client
        .post(format!("{}/{}", BACKEND_URL, "v1/group"))
        .json(&json!({
          "art": BASE64_STANDARD.encode(art.serialize()?),
          "chatId": chat_id,
          "isPrivate": is_private
        }))
        .send()
        .await?;

    assert_eq!(init_response.status(), StatusCode::CREATED);

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
    context: &mut ARTTestContext,
    metadata: Option<Vec<u8>>,
    payload: Option<Vec<u8>>,
) -> reqwest::Result<reqwest::Response> {
    let secret_key = context.art.secret_key.clone();
    let new_secret_key = ARTScalarField::rand(&mut context.rng);

    // let mut associated_data = Vec::new();
    // context
    //     .art
    //     .root
    //     .public_key
    //     .serialize_uncompressed(&mut associated_data)
    //     .unwrap();

    let (_, key_update_changes) = context.art.update_key(&new_secret_key).unwrap();
    let (_, artefacts) = context.art.recompute_root_key_with_artefacts().unwrap();

    let mut req = GroupOperationRequest {
        branch_changes: key_update_changes.serialze().unwrap(),
        proof: vec![],
        payload,
    };

    let associated_data = req.get_aux_data();

    let blindings: Vec<_> = (0..artefacts.co_path.len() + 1)
        .map(|_| Scalar::random(&mut thread_rng()))
        .collect();

    let public_key = CortadoAffine::generator().mul(secret_key).into_affine();

    let proof = art_prove(
        context.basis.clone(),
        associated_data.as_slice(),
        vec![public_key],
        artefacts.path.clone(),
        artefacts.co_path.clone(),
        artefacts.secrets.clone(),
        vec![secret_key],
        blindings,
    )
    .unwrap();

    let verification_result = art_verify(
        context.basis.clone(),
        associated_data.as_slice(),
        vec![public_key],
        key_update_changes
            .public_keys
            .iter()
            .rev()
            .cloned()
            .collect(),
        context
            .art
            .get_co_path_values(&key_update_changes.node_index.get_path().unwrap())
            .unwrap(),
        proof.clone(),
    )
    .is_ok();

    assert_eq!(verification_result, true);

    let mut proof_bytes = Vec::new();
    proof.serialize_uncompressed(&mut proof_bytes).unwrap();

    req.proof = proof_bytes;
    context
        .client
        .put(format!(
            "{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid
        ))
        .json(&req)
        .send()
        .await
}

// add node to the art, and send updates to the chat
async fn add_member(context: &mut ARTTestContext) -> reqwest::Result<reqwest::Response> {
    let old_tk = context.art.recompute_root_key().unwrap().key;
    let new_user_secret_key = ARTScalarField::rand(&mut context.rng);
    let (_, append_user_changes) = context.art.append_node(&new_user_secret_key).unwrap();
    let (_, artefacts) = context
        .art
        .recompute_root_key_with_artefacts_using_secret_key(
            new_user_secret_key,
            Some(&append_user_changes.node_index),
        )
        .unwrap();

    let blindings: Vec<_> = (0..artefacts.co_path.len() + 1)
        .map(|_| Scalar::random(&mut thread_rng()))
        .collect();

    let public_key = CortadoAffine::generator().mul(old_tk).into_affine();

    let mut req = GroupOperationRequest {
        branch_changes: append_user_changes.serialze().unwrap(),
        proof: vec![],
        payload: None,
    };

    let mut associated_data = req.get_aux_data();

    let proof = art_prove(
        context.basis.clone(),
        associated_data.as_slice(),
        vec![public_key],
        artefacts.path.clone(),
        artefacts.co_path.clone(),
        artefacts.secrets.clone(),
        vec![old_tk],
        blindings,
    )
    .unwrap();
    let verification_result = art_verify(
        context.basis.clone(),
        associated_data.as_slice(),
        vec![public_key],
        append_user_changes
            .public_keys
            .iter()
            .rev()
            .cloned()
            .collect(),
        context
            .art
            .get_co_path_values(&append_user_changes.node_index.get_path().unwrap())
            .unwrap(),
        proof.clone(),
    )
    .is_ok();

    assert_eq!(verification_result, true);

    let mut proof_bytes = Vec::new();
    proof.serialize_uncompressed(&mut proof_bytes).unwrap();

    req.proof = proof_bytes;

    context
        .client
        .put(format!(
            "{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid
        ))
        .json(&req)
        .send()
        .await
}

async fn make_blank(
    context: &mut ARTTestContext,
    member_id: usize,
) -> reqwest::Result<reqwest::Response> {
    let old_tk = context.art.recompute_root_key().unwrap().key;
    let user_to_remove = ARTGroup::generator()
        .mul(&context.initial_secrets[member_id])
        .into_affine();
    let temporary_secret_key = ARTScalarField::rand(&mut context.rng);
    let (_, remove_user_changes) = context
        .art
        .make_blank(&user_to_remove, &temporary_secret_key)
        .unwrap();
    let (_, artefacts) = context
        .art
        .recompute_root_key_with_artefacts_using_secret_key(
            temporary_secret_key,
            Some(&remove_user_changes.node_index),
        )
        .unwrap();

    let blindings: Vec<_> = (0..artefacts.co_path.len() + 1)
        .map(|_| Scalar::random(&mut thread_rng()))
        .collect();

    let old_tk_pub = CortadoAffine::generator().mul(&old_tk).into_affine();

    let mut req = GroupOperationRequest {
        branch_changes: remove_user_changes.serialze().unwrap(),
        proof: vec![],
        payload: None,
    };

    let associated_data = req.get_aux_data();

    let proof = art_prove(
        context.basis.clone(),
        associated_data.as_slice(),
        vec![old_tk_pub],
        artefacts.path.clone(),
        artefacts.co_path.clone(),
        artefacts.secrets.clone(),
        vec![old_tk],
        blindings,
    )
    .unwrap();

    let verification_result = art_verify(
        context.basis.clone(),
        associated_data.as_slice(),
        vec![old_tk_pub],
        remove_user_changes
            .public_keys
            .iter()
            .rev()
            .cloned()
            .collect(),
        context
            .art
            .get_co_path_values(&remove_user_changes.node_index.get_path().unwrap())
            .unwrap(),
        proof.clone(),
    )
    .is_ok();

    assert_eq!(verification_result, true);

    let mut proof_bytes = Vec::new();
    proof.serialize_uncompressed(&mut proof_bytes).unwrap();

    req.proof = proof_bytes;

    context
        .client
        .put(format!(
            "{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid
        ))
        .json(&req)
        .send()
        .await
}

async fn get_art(
    context: &mut ARTTestContext,
    sequence_number: i64,
) -> reqwest::Result<reqwest::Response> {
    // Get challenge for proof
    let challenge_response = get_challenge(context).await?;
    assert_eq!(challenge_response.status(), StatusCode::OK);

    let challenge = BASE64_STANDARD
        .decode(challenge_response.text().await.unwrap())
        .unwrap();

    // Second try to get challenge, to test that it is different
    let challenge2_response = get_challenge(context).await?;
    assert_eq!(challenge2_response.status(), StatusCode::OK);

    let challenge2 = BASE64_STANDARD
        .decode(challenge2_response.text().await.unwrap())
        .unwrap();

    assert_ne!(challenge2, challenge);

    // Create signature
    let nonce = (0..DEFAULT_NONCE_LENGTH)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();

    let mut msg = Vec::new();
    msg.extend_from_slice(context.chat_uuid.as_bytes());
    msg.extend(&nonce);
    msg.extend(&challenge);

    let pk = vec![context.art.public_key_of(&context.art.secret_key)];

    let signature = sign(&vec![context.art.secret_key], &pk, &msg).unwrap();
    let verification_result = verify(&signature, &pk, &msg);
    assert!(verification_result.is_ok());

    let mut public_key_bytes = Vec::new();
    pk[0].serialize_uncompressed(&mut public_key_bytes).unwrap();

    context
        .client
        .get(format!(
            "{}/{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid, sequence_number
        ))
        .query(&GetARTQuery {
            signature,
            nonce,
            challenge,
            public_key: public_key_bytes,
        })
        .send()
        .await
}

async fn get_challenge(context: &mut ARTTestContext) -> reqwest::Result<reqwest::Response> {
    let mut serialized_public_key = Vec::new();
    context
        .art
        .public_key_of(&context.art.secret_key)
        .serialize_uncompressed(&mut serialized_public_key);

    context
        .client
        .get(format!(
            "{}/{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid, "challenge"
        ))
        .send()
        .await
}

async fn get_changes(
    mut context: &mut ARTTestContext,
    limit: i64,
    skip: i64,
    epoch: i64,
) -> reqwest::Result<reqwest::Response> {
    let tk = context.art.recompute_root_key().unwrap().key;
    let pk = context.art.root.public_key;

    let mut msg = Vec::new();
    let nonce = (0..DEFAULT_NONCE_LENGTH)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();
    msg.extend_from_slice(context.chat_uuid.as_bytes());
    msg.extend(&nonce);

    let signature = sign(&vec![tk], &vec![pk], &msg).unwrap();

    assert!(verify(&signature, &vec![pk], &msg).is_ok());

    context
        .client
        .get(format!(
            "{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid
        ))
        .query(&GetChangesQuery {
            signature,
            nonce,
            limit: limit as i64,
            skip: skip as i64,
            epoch: epoch as i64,
        })
        .send()
        .await
}
