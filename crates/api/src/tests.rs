use ark_ec::{AffineRepr, CurveGroup};
use ark_ed25519::EdwardsAffine as Ed25519Affine;
use ark_serialize::CanonicalSerialize;
use ark_std::{
    UniformRand,
    rand::prelude::StdRng,
    rand::{SeedableRng, thread_rng},
};
use art::types::BranchChanges;
use art::{
    errors::ARTError,
    traits::{ARTPrivateAPI, ARTPrivateView, ARTPublicAPI, ARTPublicView},
    types::{PrivateART, PublicART},
};
use axum::body::Bytes;
use base64::{Engine, prelude::BASE64_STANDARD};
use bulletproofs::PedersenGens;
use bytes::{BufMut, BytesMut};
use cortado::{CortadoAffine, Fr};
use crypto::schnorr::{sign, verify};
use curve25519_dalek::Scalar;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use mongodb::bson::doc;
use prost::Message;
use rand::Rng;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use serde_with::{base64::Base64, serde_as};
use std::{collections::HashMap, ops::Mul, time::Duration};
use tracing::debug;
use tracing::field::debug;
use types::messenger_schemas::GetMessageQuery;
use types::protos::{Frame, SpFrames, group_operation::Operation};
use types::{art_schemas::*, centrifugo_schemas::*, messenger_schemas::*, protos};
use uuid::Uuid;
use zk::art::{art_prove, art_verify};
use zkp::toolbox::{cross_dleq::PedersenBasis, dalek_ark::ristretto255_to_ark};

const BACKEND_URL: &str = "http://localhost:8080";
const CENTRIFUGO_URL: &str = "http://localhost:8000";
// used for tests, which can be repeated
const TEST_REPEATS: usize = 4;
const DEFAULT_NONCE_LENGTH: u32 = 16; // 16 bytes
const DEFAULT_GROUP_SIZE: u64 = 10;

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
    pub art: PrivateART<CortadoAffine>,
    pub initial_secrets: Vec<Fr>,
    pub rng: StdRng,
    pub chat_uuid: Uuid,
    pub basis: PedersenBasis<CortadoAffine, Ed25519Affine>,
}

impl ARTTestContext {
    pub async fn new(size: u64) -> (Self, BytesMut) {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let secrets = (0..size).map(|_| Fr::rand(&mut rng)).collect();
        let (art, _) =
            PrivateART::new_art_from_secrets(&secrets, &CortadoAffine::generator()).unwrap();

        // Create new_group for testing
        let (chat_uuid, init_message) = crate_new_chat(
            PublicART::new_art_from_secrets(&secrets, &CortadoAffine::generator())
                .unwrap()
                .0,
        )
        .await
        .unwrap();

        (
            Self {
                client: reqwest::Client::new(),
                art,
                initial_secrets: secrets,
                rng,
                chat_uuid,
                basis: get_pedersen_basis(),
            },
            init_message,
        )
    }

    pub fn derive_new(&self, index: i64) -> Result<Self, ARTError> {
        let (mut art, _) =
            PrivateART::new_art_from_secrets(&self.initial_secrets, &CortadoAffine::generator())?;
        art.secret_key = self.initial_secrets[index as usize].clone();
        art.update_node_index()?;

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

#[tokio::test]
async fn test_send_message() -> eyre::Result<()> {
    init_tracing_for_test();

    let (mut context, init_message) = ARTTestContext::new(DEFAULT_GROUP_SIZE).await;

    let challenge_response = get_challenge(&mut context).await?;
    assert_eq!(challenge_response.status(), StatusCode::OK);

    let challenge = challenge_response
        .json::<ChallengeResponse>()
        .await?
        .challenge;

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

    let tbs_frame = protos::FrameTbs {
        group_id: context.chat_uuid.to_string(),
        epoch: 0,
        nonce: (0..DEFAULT_NONCE_LENGTH)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<u8>>(),
        group_operation: None,
        protected_payload: "zk messenger is the best".as_bytes().to_vec(),
    };
    let mut buf = BytesMut::new();
    tbs_frame.encode(&mut buf)?;

    let tk = context.art.recompute_root_key()?.key;
    let pk = vec![context.art.root.public_key];

    let signature = sign(&vec![tk], &pk, &buf)?;
    let verification_result = verify(&signature, &pk, &buf);
    assert!(verification_result.is_ok());

    let req = protos::Frame {
        frame: Some(tbs_frame),
        proof: signature,
    };

    let mut req_buf = BytesMut::new();
    req.encode(&mut req_buf)?;
    let test_message = req_buf.to_vec();

    let handle = tokio::spawn(async move {
        while let Some(event_result) = stream.next().await {
            match event_result {
                Ok(centrifugo_event) => {
                    match centrifugo_event {
                        CentrifugoEvent::Connect(_connect_msg) => {
                            // Connection established, continue waiting for messages
                            debug!(
                                "send_message: Connection established, continue waiting for messages"
                            );
                            // debug!("send_message: _connect_msg: {:#?}", _connect_msg.connect);
                        }
                        CentrifugoEvent::ChannelMessage(channel_msg) => {
                            let content_string = channel_msg.publication.data.content;

                            if content_string.eq(&init_message.to_vec()) {
                                // skip accidental init group message
                                continue;
                            }

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
    context
        .client
        .post(format!(
            "{}/{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid, "frame"
        ))
        .body(Bytes::from(req_buf))
        .send()
        .await?;

    let result = tokio::time::timeout(Duration::from_secs(5), handle).await?;

    if let Err(e) = result {
        panic!("Error: {:?}", e);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_message() -> eyre::Result<()> {
    init_tracing_for_test();

    let (mut context, init_message) = ARTTestContext::new(DEFAULT_GROUP_SIZE).await;
    let mut test_context = context.derive_new(2)?;

    let mut messages = Vec::with_capacity(TEST_REPEATS + 1);
    // messages.push(init_message);
    for _ in 0..TEST_REPEATS {
        let (add_member_response, message) = add_member(&mut context).await?;
        messages.push(message);
        assert_eq!(add_member_response.status(), StatusCode::OK);
    }

    let get_messages_response = get_messages(&mut test_context, TEST_REPEATS as i64, 1).await?;
    assert_eq!(get_messages_response.status(), StatusCode::ACCEPTED);
    // frame.encode(&mut buf).unwrap();
    let mut buf = BytesMut::from(&*get_messages_response.bytes().await?);
    let sp_frames = SpFrames::decode(buf)?;

    for (sp_frame, message) in sp_frames.sp_frames.iter().zip(messages.iter()) {
        let frame_message = Frame::decode(&**message)?;
        assert_eq!(frame_message, sp_frame.frame.clone().unwrap());
    }

    Ok(())
}

#[tokio::test]
async fn test_add_member() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = ARTTestContext::new(DEFAULT_GROUP_SIZE).await.0;

    for _ in 0..TEST_REPEATS {
        let add_member_response = add_member(&mut context).await?.0;

        assert_eq!(add_member_response.status(), StatusCode::OK);
    }

    Ok(())
}

#[tokio::test]
async fn test_add_member_after_removal() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = ARTTestContext::new(DEFAULT_GROUP_SIZE).await.0;

    for i in 0..TEST_REPEATS {
        make_blank(&mut context, i + 1).await?;
        let add_member_response = add_member(&mut context).await?.0;

        assert_eq!(add_member_response.status(), StatusCode::OK);
    }

    Ok(())
}

#[tokio::test]
async fn test_remove_member() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = ARTTestContext::new(DEFAULT_GROUP_SIZE).await.0;
    let mut retrieval_context = context.derive_new(2)?;

    for i in 1..TEST_REPEATS + 1 {
        // skip the root node
        let remove_user_response = make_blank(&mut context, i).await?;
        assert_eq!(remove_user_response.status(), StatusCode::NO_CONTENT);

        let new_art_response = get_art(&mut retrieval_context, i as i64, None, ProofMode::UseLeafKey.to_string()).await?;
        assert_eq!(new_art_response.status(), StatusCode::OK);

        let received_art = PublicART::<CortadoAffine>::deserialize(
            &new_art_response.json::<GetARTResponse>().await?.art,
        )?;

        assert_eq!(
            received_art.root.weight,
            retrieval_context.art.root.weight - 1
        );

        retrieval_context.art = PrivateART::from_public_art(received_art, context.art.secret_key)?;

        let sk_to_use = retrieval_context.art.recompute_root_key()?.key;
        let new_art_check_response =
            get_art(&mut retrieval_context, i as i64, Some(sk_to_use), ProofMode::UseRootKey.to_string()).await?;
        assert_eq!(new_art_check_response.status(), StatusCode::OK);
        let received_art_check = PublicART::<CortadoAffine>::deserialize(
            &new_art_check_response.json::<GetARTResponse>().await?.art,
        )?;
        assert_eq!(received_art_check.root, retrieval_context.art.root);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_art() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = ARTTestContext::new(DEFAULT_GROUP_SIZE).await.0;
    let mut retrieval_context = context.derive_new(2)?;
    let mut art_roots = vec![context.art.root.public_key];

    // update art several times, so we can retrieve them
    for i in 0..TEST_REPEATS {
        let key_update_response = update_key(&mut context, None, i as u64).await?;
        art_roots.push(context.art.root.public_key);

        assert_eq!(key_update_response.status(), StatusCode::OK);
    }

    // Test if retrieval is correct
    for i in 0..TEST_REPEATS {
        let art_response = get_art(&mut retrieval_context, i as i64, None, ProofMode::UseLeafKey.to_string()).await?;

        assert_eq!(art_response.status(), StatusCode::OK);

        let received_art = PublicART::<CortadoAffine>::deserialize(
            &art_response.json::<GetARTResponse>().await?.art,
        )?;

        assert_eq!(received_art.root.public_key, art_roots[(i) as usize]);
        retrieval_context.art =
            PrivateART::from_public_art(received_art, retrieval_context.initial_secrets[1])?;
    }

    Ok(())
}

fn new_nonce() -> Vec<u8> {
    (0..DEFAULT_NONCE_LENGTH)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>()
}

#[tokio::test]
async fn test_delete_chat() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = ARTTestContext::new(DEFAULT_GROUP_SIZE).await.0;
    let nonce = new_nonce();

    let challenge_response = get_challenge(&mut context).await?;
    assert_eq!(challenge_response.status(), StatusCode::OK);
    let challenge = challenge_response
        .json::<ChallengeResponse>()
        .await?
        .challenge;

    let tbs_frame = protos::FrameTbs {
        group_id: context.chat_uuid.to_string(),
        epoch: 0,
        nonce,
        group_operation: Some(protos::GroupOperation {
            operation: Some(protos::group_operation::Operation::DropGroup(challenge)),
        }),
        protected_payload: vec![],
    };

    let mut buf = BytesMut::new();
    tbs_frame.encode(&mut buf).unwrap();
    let msg = &*buf;

    let pk = vec![context.art.public_key_of(&context.art.secret_key)];
    let signature = sign(&vec![context.art.secret_key], &pk, &msg).unwrap();
    let verification_result = verify(&signature, &pk, &msg);
    assert!(verification_result.is_ok());

    let mut req = protos::Frame {
        frame: Some(tbs_frame),
        proof: signature,
    };

    let mut buf = BytesMut::new();
    req.encode(&mut buf).unwrap();

    let delete_response = context
        .client
        .post(format!(
            "{}/{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid, "frame"
        ))
        .body(Bytes::from(buf))
        .send()
        .await?;

    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    Ok(())
}

async fn crate_new_chat(art: PublicART<CortadoAffine>) -> eyre::Result<(Uuid, BytesMut)> {
    let chat_id = Uuid::now_v7();
    let client = reqwest::Client::new();

    let req = protos::Frame {
        frame: Some(protos::FrameTbs {
            group_id: chat_id.to_string(),
            epoch: 0,
            nonce: vec![],
            group_operation: Some(protos::GroupOperation {
                operation: Some(Operation::Init(art.serialize()?)),
            }),
            protected_payload: vec![],
        }),
        proof: vec![],
    };

    let mut buf = BytesMut::new();
    req.encode(&mut buf)?;
    let init_message = buf.clone();

    let init_response = client
        .post(format!(
            "{}/{}/{}/{}",
            BACKEND_URL, "v1/group", chat_id, "frame"
        ))
        .body(Bytes::from(buf))
        .send()
        .await?;

    assert_eq!(init_response.status(), StatusCode::CREATED);

    Ok((chat_id, init_message))
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

// update key in art, and send updates to the chat
async fn update_key(
    context: &mut ARTTestContext,
    payload: Option<Vec<u8>>,
    epoch: u64,
) -> reqwest::Result<reqwest::Response> {
    let secret_key = context.art.secret_key.clone();
    let new_secret_key = Fr::rand(&mut context.rng);

    let (_, key_update_changes) = context.art.update_key(&new_secret_key).unwrap();
    let (_, artefacts) = context.art.recompute_root_key_with_artefacts().unwrap();

    let tbs_frame = protos::FrameTbs {
        group_id: context.chat_uuid.to_string(),
        epoch,
        nonce: vec![],
        group_operation: Some(protos::GroupOperation {
            operation: Some(Operation::KeyUpdate(key_update_changes.serialze().unwrap())),
        }),
        protected_payload: payload.unwrap_or(vec![]),
    };

    let mut buf = BytesMut::new();
    tbs_frame.encode(&mut buf).unwrap();
    let associated_data = &*buf;

    let blindings: Vec<_> = (0..artefacts.co_path.len() + 1)
        .map(|_| Scalar::random(&mut thread_rng()))
        .collect();

    let public_key = CortadoAffine::generator().mul(secret_key).into_affine();

    let proof = art_prove(
        context.basis.clone(),
        associated_data,
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
        associated_data,
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

    let mut req = protos::Frame {
        frame: Some(tbs_frame),
        proof: proof_bytes,
    };

    let mut buf = BytesMut::new();
    req.encode(&mut buf).unwrap();

    context
        .client
        .post(format!(
            "{}/{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid, "frame"
        ))
        .body(Bytes::from(buf))
        .send()
        .await
}

// add node to the art, and send updates to the chat
async fn add_member(
    context: &mut ARTTestContext,
) -> reqwest::Result<(reqwest::Response, BytesMut)> {
    let old_tk = context.art.recompute_root_key().unwrap().key;
    let new_user_secret_key = Fr::rand(&mut context.rng);
    let (_, append_user_changes) = context.art.append_node(&new_user_secret_key).unwrap();
    let (_, artefacts) = context
        .art
        .recompute_root_key_with_artefacts_using_secret_key(
            new_user_secret_key,
            Some(&append_user_changes.node_index),
        )
        .unwrap();

    let tbs_frame = protos::FrameTbs {
        group_id: context.chat_uuid.to_string(),
        epoch: 0,
        nonce: vec![],
        group_operation: Some(protos::GroupOperation {
            operation: Some(protos::group_operation::Operation::AddMember(
                append_user_changes.serialze().unwrap(),
            )),
        }),
        protected_payload: vec![],
    };

    let blindings: Vec<_> = (0..artefacts.co_path.len() + 1)
        .map(|_| Scalar::random(&mut thread_rng()))
        .collect();

    let public_key = CortadoAffine::generator().mul(old_tk).into_affine();

    let mut buf = BytesMut::new();
    tbs_frame.encode(&mut buf).unwrap();
    let associated_data = &*buf;

    let proof = art_prove(
        context.basis.clone(),
        associated_data,
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
        associated_data,
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

    let mut req = protos::Frame {
        frame: Some(tbs_frame),
        proof: proof_bytes,
    };

    let mut buf = BytesMut::new();
    req.encode(&mut buf).unwrap();

    Ok((
        context
            .client
            .post(format!(
                "{}/{}/{}/{}",
                BACKEND_URL, "v1/group", context.chat_uuid, "frame"
            ))
            .body(Bytes::from(buf.clone()))
            .send()
            .await?,
        buf,
    ))
}

async fn get_messages(
    context: &mut ARTTestContext,
    limit: i64,
    skip: i64,
) -> reqwest::Result<reqwest::Response> {
    let sk = context.art.recompute_root_key().unwrap().key;
    let pk = context.art.public_key_of(&sk);

    let nonce = new_nonce();

    let mut msg = Vec::new();
    msg.extend_from_slice(context.chat_uuid.as_bytes());
    msg.extend(&nonce);

    let signature = sign(&vec![sk], &vec![pk], &msg).unwrap();
    let verification_result = verify(&signature, &vec![pk], &msg);
    assert!(verification_result.is_ok());

    context
        .client
        .get(format!(
            "{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid
        ))
        .query(&GetMessageQuery {
            message_sequence_number: None,
            limit,
            skip,
            signature,
            nonce,
            epoch: None,
        })
        .send()
        .await
}

async fn make_blank(
    context: &mut ARTTestContext,
    member_id: usize,
) -> reqwest::Result<reqwest::Response> {
    let old_tk = context.art.recompute_root_key().unwrap().key;
    let user_to_remove = CortadoAffine::generator()
        .mul(&context.initial_secrets[member_id])
        .into_affine();
    let temporary_secret_key = Fr::rand(&mut context.rng);
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

    let tbs_frame = protos::FrameTbs {
        group_id: context.chat_uuid.to_string(),
        epoch: 0,
        nonce: vec![],
        group_operation: Some(protos::GroupOperation {
            operation: Some(protos::group_operation::Operation::RemoveMember(
                remove_user_changes.serialze().unwrap(),
            )),
        }),
        protected_payload: vec![],
    };

    let blindings: Vec<_> = (0..artefacts.co_path.len() + 1)
        .map(|_| Scalar::random(&mut thread_rng()))
        .collect();

    let old_tk_pub = CortadoAffine::generator().mul(&old_tk).into_affine();

    let mut buf = BytesMut::new();
    tbs_frame.encode(&mut buf).unwrap();
    let associated_data = &*buf;

    let proof = art_prove(
        context.basis.clone(),
        associated_data,
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
        associated_data,
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

    let mut req = protos::Frame {
        frame: Some(tbs_frame),
        proof: proof_bytes,
    };

    let mut buf = BytesMut::new();
    req.encode(&mut buf).unwrap();

    context
        .client
        .post(format!(
            "{}/{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid, "frame"
        ))
        .body(Bytes::from(buf))
        .send()
        .await
}

async fn get_art(
    context: &mut ARTTestContext,
    epoch: i64,
    secret_key_to_use: Option<Fr>,
    proof_mode: String,
) -> reqwest::Result<reqwest::Response> {
    // Get challenge for proof
    let challenge_response = get_challenge(context).await?;
    assert_eq!(challenge_response.status(), StatusCode::OK);

    let challenge = challenge_response
        .json::<ChallengeResponse>()
        .await?
        .challenge;

    // Second try to get challenge, to test that it is different
    let challenge2_response = get_challenge(context).await?;
    assert_eq!(challenge2_response.status(), StatusCode::OK);

    let challenge2 = challenge2_response
        .json::<ChallengeResponse>()
        .await?
        .challenge;

    assert_ne!(challenge2, challenge);

    // Create signature
    let nonce = (0..DEFAULT_NONCE_LENGTH)
        .map(|_| rand::random::<u8>())
        .collect::<Vec<u8>>();

    let mut msg = Vec::new();
    msg.extend_from_slice(context.chat_uuid.as_bytes());
    msg.extend(&nonce);
    msg.extend(&challenge);
    msg.extend(epoch.to_be_bytes());

    let sk = match secret_key_to_use {
        Some(secret_key) => secret_key,
        None => context.art.secret_key,
    };

    let pk = context.art.public_key_of(&sk);

    let signature = sign(&vec![sk], &vec![pk], &msg).unwrap();
    let verification_result = verify(&signature, &vec![pk], &msg);
    assert!(verification_result.is_ok());

    let mut public_key_bytes = Vec::new();
    pk.serialize_uncompressed(&mut public_key_bytes).unwrap();

    context
        .client
        .get(format!(
            "{}/{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid, epoch
        ))
        .query(&GetARTQuery {
            signature,
            nonce,
            challenge,
            proof_mode,
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
        .serialize_uncompressed(&mut serialized_public_key)
        .unwrap();

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
    context: &mut ARTTestContext,
    limit: i64,
    skip: i64,
    epoch: i64,
) -> reqwest::Result<Vec<BranchChanges<CortadoAffine>>> {
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

    let changes_response = context
        .client
        .get(format!(
            "{}/{}/{}",
            BACKEND_URL, "v1/group", context.chat_uuid
        ))
        .query(&json!({
            "signature": BASE64_STANDARD.encode(&signature),
            "nonce": BASE64_STANDARD.encode(&nonce),
            "limit": limit,
            "skip": skip,
            "epoch": epoch
        }))
        .send()
        .await?;

    assert_eq!(changes_response.status(), StatusCode::OK);

    let records = changes_response.json::<Vec<types::MessageRecord>>().await?;
    let mut changes = Vec::with_capacity(records.len());
    for record in records {
        let mut buf = BytesMut::new();
        buf.put(&*record.content);
        let frame = Frame::decode(buf).unwrap();
        let frame_change = match frame
            .frame
            .unwrap()
            .group_operation
            .unwrap()
            .operation
            .unwrap()
        {
            Operation::Init(_) => None,
            Operation::AddMember(change)
            | Operation::KeyUpdate(change)
            | Operation::RemoveMember(change) => {
                Some(BranchChanges::<CortadoAffine>::deserialize(change.as_slice()).unwrap())
            }
            Operation::DropGroup(_) => None,
        };
        if let Some(frame_change) = frame_change {
            changes.push(frame_change)
        }
    }

    Ok(changes)
}

fn init_tracing_for_test() {
    _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
}
