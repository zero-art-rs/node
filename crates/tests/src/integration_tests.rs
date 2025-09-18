use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::{thread, time};
use std::time::Duration;
use crate::{
    user_integration_test_model::UserIntegrationTestModel,
    init_tracing_for_test
};
use ark_std::rand::SeedableRng;
use ark_std::rand::prelude::StdRng;
use art::traits::{ARTPrivateAPI, ARTPrivateView, ARTPublicAPI, ARTPublicView};
use art::types::{PrivateART, PublicART};
use axum::http::StatusCode;
use axum::Router;
use axum::routing::{get, post};
use axum_test::TestServer;
use bytes::{Bytes, BytesMut};
use cortado::CortadoAffine;
use crypto::schnorr::{sign, verify};
use eventsource_stream::Eventsource;
use prost::Message;
use tracing::{debug, info};
use tracing::field::debug;
use crate::utils::{CentrifugoTokenResponse, CentrifugoEvent};
use types::art_schemas::{ChallengeResponse, GetARTResponse, ProofMode};
use types::centrifugo_schemas::AuthRequest;
use types::protos;
use types::protos::{Frame, SpFrames};
use futures_util::StreamExt;
use mongodb::bson::doc;
use mongodb::Client;
use mongodb::options::ClientOptions;
use sha3::{Sha3_256, Digest};
use tokio::sync::{mpsc, RwLock};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;
use api::{ARTService, CentrifugoService, Container, MessengerService};
use proof_verifier::ProofVerifierSender;
use crate::sender::{Sender, TestSender, TestServerSender};
use api::{centrifugo_transport, art_transport, messenger_transport};
use storage::DATABASE;

const BACKEND_URL: &str = "http://localhost:8080";
const CENTRIFUGO_URL: &str = "http://localhost:8000";
// used for tests, which can be repeated
const TEST_REPEATS: usize = 4;
const DEFAULT_NONCE_LENGTH: u32 = 16; // 16 bytes
const DEFAULT_GROUP_SIZE: u64 = 10;


fn get_test_sender() -> TestSender {
    TestSender {
        backend_url: BACKEND_URL.to_string(),
        centrifugo_url: CENTRIFUGO_URL.to_string(),
        client: reqwest::Client::new(),
    }
}

#[derive(utoipa::OpenApi)]
#[openapi(
    info(title = env!("CARGO_PKG_NAME"),),
    components(schemas(
        types::centrifugo_schemas::AuthRequest,
        types::centrifugo_schemas::AuthResponse,
    )),
)]
struct PublicApiDoc;

pub fn build_test_server(container: Arc<Container>) -> TestServer {
    // let health_handler_route = OpenApiRouter::new().routes(routes![get_health_handler]);

    //Messages:
    let list_messages_route: Router = Router::new()
        .route("/v1/group/{id}/frames", get(messenger_transport::list_messages))
        .with_state(container.clone());
        // .with_verification(container.clone())
        // .with_route_id("list_messages");

    let count_messages_route = OpenApiRouter::new()
        .route("/v1/group/{id}/frames/count", get(messenger_transport::count_messages))
        .with_state(container.clone());
        // .with_verification(container.clone())
        // .with_route_id("count_messages");

    // Centrifugo:
    let authenticate_route = OpenApiRouter::new()
        .route("/centrifugo/auth", post(centrifugo_transport::authenticate))
        .with_state(container.clone());
        // .with_verification(container.clone())
        // .with_route_id("authenticate");

    // Group management:
    let send_frame_route = OpenApiRouter::new()
        .route("/v1/group/{id}/frames", post(messenger_transport::send_frame))
        .with_state(container.clone());
        // .with_verification(container.clone())
        // .with_route_id("send_frame");

    let get_art_route = OpenApiRouter::new()
        .route("/v1/group/{id}/{epoch}", get(art_transport::get_art))
        .with_state(container.clone());
        // .with_verification(container.clone())
        // .with_route_id("get_art");

    let get_challenge_route = OpenApiRouter::new()
        .route("/v1/group/{id}/challenge", get(art_transport::get_challenge))
        .with_state(container.clone());

    // Combine routers in one OpenApiRouter
    let router = Router::default()
        // .merge(health_handler_route)
        // Messages
        .merge(list_messages_route)
        .merge(count_messages_route)
        // Centrifugo
        .merge(authenticate_route)
        // Group operations
        .merge(send_frame_route)
        .merge(get_art_route)
        .merge(get_challenge_route);

    let test_server = TestServer::new(router).unwrap();

    test_server
}

async fn get_test_server() -> Rc<TestServer> {
    let (proof_verifier_tx, proof_verifier_rx) = mpsc::channel(1000);

    let uri = "mongodb://127.0.0.1:27017/zkmsngr?replicaSet=rs0&directConnection=true";
    let database_name = "zkmsngr";

    let client_options = ClientOptions::parse(uri).await.unwrap();
    let client = Client::with_options(client_options).unwrap();

    match DATABASE
        .set(
            client
                .default_database()
                .unwrap_or(client.database(&database_name)),
        ) {
        Ok(_) => debug!("Database set successfully"),
        Err(_) => debug!("Database is already set"),
    };

    thread::sleep(Duration::from_millis(1000));

    debug!("Try to ping database");
    DATABASE
        .get()
        .unwrap()
        .run_command(doc! { "ping": 1 })
        .await.unwrap();
    info!("Connected to database {}", database_name);

    let messenger_service = MessengerService::new();
    let centrifugo_service = CentrifugoService::new(
        String::from("tkE0hTS953BL3ETHeFyHGY3cAl78xyGCdPCtsGIX-oiyJ_Suz_ui_j3Gjrp8JP62Lq8tHCoih6rBMeUvfGPOvw"),
        Duration::new(86400, 0),
        vec![String::from("personal")],
    );
    let art_service = ARTService::new();


    let container = Arc::new(Container {
        messenger_service: Arc::new(messenger_service),
        centrifugo_service: Arc::new(centrifugo_service),
        art_service: Arc::new(art_service),
        proof_verifier_sender: proof_verifier_tx,
        challenges: Arc::new(RwLock::new(HashSet::new())),
    });

    let test_server = build_test_server(container);

    Rc::new(test_server)
}

async fn get_test_server_sender() -> TestServerSender {
    let test_server = get_test_server().await;

    let sender = TestServerSender {
        backend_url: BACKEND_URL.to_string(),
        centrifugo_url: CENTRIFUGO_URL.to_string(),
        test_server,
    };

    sender
}

async fn get_sender() -> impl Sender {
    // let sender = get_test_server_sender().await;
    let sender = get_test_sender();

    return sender;
}

#[tokio::test]
async fn test_send_message() -> eyre::Result<()> {
    init_tracing_for_test();

    let sender = get_test_sender();

    let (context, init_message) = UserIntegrationTestModel::new(
        DEFAULT_GROUP_SIZE,
        sender
    ).await;

    let challenge = Sha3_256::digest(context.get_challenge().await?).to_vec();

    let nonce = std::iter::repeat(rand::random::<u8>())
        .take(DEFAULT_NONCE_LENGTH as usize)
        .collect::<Vec<u8>>();

    let tk = context.art.get_root_key().unwrap().key;
    let pk = context.art.get_root().public_key;

    let signature = sign(&vec![tk], &vec![pk], &challenge).unwrap();

    // Get Centrifugo auth token
    let auth_request = AuthRequest {
        chat_ids: vec![context.chat_uuid],
        proof: signature,
        nonce,
        challenge,
        epochs: vec![0],
    };

    let centrifugo_token_response = context
        .sender
        .get_centrifugo_token(auth_request)
        .await?;

    let url = format!(
        "{}/{}",
        CENTRIFUGO_URL,
        "connection/uni_sse?cf_connect={\"token\":\"".to_owned()
            + &centrifugo_token_response.token
            + "\"}"
    );

    let response = context.sender.client.get(&url).send().await?;

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

    let msg = Sha3_256::digest(tbs_frame.encode_to_vec()).to_vec();
    let tk = context.art.get_root_key()?.key;
    let pk = vec![context.art.root.public_key];

    let signature = sign(&vec![tk], &pk, &msg)?;
    let verification_result = verify(&signature, &pk, &msg);
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

    context.sender.send_frame(req, context.chat_uuid, None).await?;

    let result = tokio::time::timeout(Duration::from_secs(5), handle).await?;

    if let Err(e) = result {
        panic!("Error: {:?}", e);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_message() -> eyre::Result<()> {
    init_tracing_for_test();

    let (mut context, _) = UserIntegrationTestModel::new(
        DEFAULT_GROUP_SIZE,
        get_sender().await,
    ).await;
    let test_context = context.derive_new(2)?;

    let mut messages = Vec::with_capacity(TEST_REPEATS + 1);
    // messages.push(init_message);
    for _ in 0..TEST_REPEATS {
        let message = context.add_member().await?;
        messages.push(message);
    }

    let sp_frames = test_context.get_messages(TEST_REPEATS as i64, 1).await?;

    for (sp_frame, message) in sp_frames.sp_frames.iter().zip(messages.iter()) {
        let frame_message = Frame::decode(&**message)?;
    }

    Ok(())
}

#[tokio::test]
async fn test_init_group() -> eyre::Result<()> {
    init_tracing_for_test();

    UserIntegrationTestModel::new(DEFAULT_GROUP_SIZE, get_sender().await,).await.0;

    Ok(())
}

#[tokio::test]
async fn test_add_member() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserIntegrationTestModel::new(DEFAULT_GROUP_SIZE, get_sender().await,).await.0;
    for _ in 0..TEST_REPEATS {
        context.add_member().await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_add_member_after_removal() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserIntegrationTestModel::new(DEFAULT_GROUP_SIZE, get_sender().await,).await.0;

    for i in 0..TEST_REPEATS {
        let path = context.index_of(i + 1).unwrap().get_path().unwrap();
        context.make_blank(&path).await?;
        context.add_member().await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_remove_member() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserIntegrationTestModel::new(DEFAULT_GROUP_SIZE, get_sender().await,).await.0;
    let mut retrieval_context = context.derive_new(1)?;

    // skip the root node
    for i in 0..TEST_REPEATS {
        let path = context.index_of(i + 5).unwrap().get_path().unwrap();
        context.make_blank(&path).await?;

        let received_art = retrieval_context
            .get_art((i + 1) as u64, None, ProofMode::UseLeafKey.to_string())
            .await?;

        assert_eq!(
            received_art.root.weight,
            retrieval_context.art.root.weight - 1
        );

        retrieval_context.art = PrivateART::from_public_art(received_art, context.art.secret_key)?;

        let sk_to_use = retrieval_context.art.get_root_key()?.key;
        let received_art_check = retrieval_context
            .get_art(
                (i + 1) as u64,
                Some(sk_to_use),
                ProofMode::UseRootKey.to_string(),
            )
            .await?;

        assert_eq!(received_art_check.root, retrieval_context.art.root);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_art() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserIntegrationTestModel::new(DEFAULT_GROUP_SIZE, get_sender().await,).await.0;
    let mut retrieval_context = context.derive_new(2)?;
    let mut art_roots = vec![context.art.root.public_key];

    // update art several times, so we can retrieve them
    for _ in 0..TEST_REPEATS {
        context.update_key(None).await?;
        art_roots.push(context.art.root.public_key);
    }

    // Test if retrieval is correct
    for i in 0..TEST_REPEATS {
        let received_art = retrieval_context
            .get_art(i as u64, None, ProofMode::UseLeafKey.to_string())
            .await?;

        assert_eq!(received_art.root.public_key, art_roots[i]);

        retrieval_context.art =
            PrivateART::from_public_art(received_art, retrieval_context.initial_secrets[1])?;
    }

    Ok(())
}

#[tokio::test]
async fn test_epoch_merge() -> eyre::Result<()> {
    init_tracing_for_test();

    let payload = std::iter::repeat(rand::random::<u8>())
        .take(DEFAULT_NONCE_LENGTH as usize)
        .collect::<Vec<u8>>();

    let (mut user0, _) = UserIntegrationTestModel::new(DEFAULT_GROUP_SIZE, get_sender().await,).await;
    let mut user1 = user0.derive_new(1)?;
    let mut user2 = user0.derive_new(2)?;
    let mut user3 = user0.derive_new(5)?;

    // sanity check
    assert_eq!(user2.art.get_root(), user0.art.get_root());
    assert_eq!(user1.art.get_root(), user0.art.get_root());
    assert_eq!(user3.art.get_root(), user0.art.get_root());

    debug!("User 0 update key ...");
    user0.add_member().await?;
    // user0.update_key(None).await?;
    debug!("User0 TK_x: {}", user0.art.root.public_key.x);

    debug!("User 1 update key ...");
    user1.update_key(Some(payload.clone())).await?;
    debug!("User1 TK_x: {}", user1.art.root.public_key.x);

    debug!("User 3 add member ...");
    user3.update_key(Some(payload.clone())).await?;
    debug!("User3 TK_x: {}", user3.art.root.public_key.x);

    let changes = user2.get_changes(20, 0, None).await?;

    debug!("User 2 merge changes locally ...");
    user2
        .art
        .recompute_path_secrets_for_observer(&changes)
        .unwrap();
    user2.art.merge(&changes)?;
    user2.epoch += 1;
    debug!("User2 MTK_x: {}", user2.art.root.public_key.x);

    assert_eq!(
        user2
            .art
            .public_key_of(&user2.art.get_root_key().unwrap().key),
        user2.art.root.public_key,
        "Check if secret on path is the same one used for art root pub key computation."
    );

    debug!("User 2 update and send update request with merge resolved");
    user2.update_key(Some(payload.clone())).await?;
    debug!("User2 TK_x: {}", user2.art.root.public_key.x);

    Ok(())
}

#[tokio::test]
async fn test_merge_for_removal() -> eyre::Result<()> {
    init_tracing_for_test();

    // let payload = UserIntegrationTestModel::new_nonce();

    let (mut user0, _) = UserIntegrationTestModel::new(DEFAULT_GROUP_SIZE, get_sender().await,).await;
    let mut user1 = user0.derive_new(5)?;
    let mut user2 = user0.derive_new(2)?;
    let mut user3 = user0.derive_new(1)?;
    debug!("User0 sk: {}", user0.art.public_key_of(&user0.art.get_secret_key()).x);

    // sanity check
    assert_eq!(user2.art.get_root(), user0.art.get_root());
    assert_eq!(user1.art.get_root(), user0.art.get_root());
    assert_eq!(user3.art.get_root(), user0.art.get_root());

    let target_node_path = user0.index_of(4).unwrap().get_path().unwrap();

    debug!("User 0 update key ...");
    user0.make_blank(&target_node_path).await?;
    // user0.update_key(None).await?;
    debug!("User0 TK_x: {}", user0.art.root.public_key.x);
    debug!("User0 tk: {}", user0.art.get_root_key().unwrap().key);

    debug!("User1 receive changes ..");
    let blank_user_0 = user1.get_changes(20, 0, None).await?;
    user1.art.update_private_art(&blank_user_0[0]).unwrap();
    user1.epoch += 1;
    debug!("User1 tk: {}", user1.art.get_root_key().unwrap().key);
    assert_eq!(user1.art.get_root(), user0.art.get_root());

    debug!("User 1 update key ...");
    user1.make_blank(&target_node_path).await?;
    debug!("User1 TK_x: {}", user1.art.root.public_key.x);
    debug!("User1 tk: {}", user1.art.get_root_key().unwrap().key);
    assert_eq!(user1.art.public_key_of(&user1.art.get_root_key()?.key), user1.art.get_root().public_key);

    debug!("User 2 receive changes ...");
    let blank_user_1 = user2.get_changes(20, 0, None).await?;
    user2.art.update_private_art(&blank_user_1[0]).unwrap();
    user2.epoch += 1;
    debug!("User2 tk: {}", user2.art.get_root_key().unwrap().key);
    // debug!("art2:\n{}", user2.art.get_root());
    assert_eq!(user2.art.get_root(), user0.art.get_root());
    assert_eq!(
        user2.art.public_key_of(&user2.art.get_root_key()?.key),
        user0.art.public_key_of(&user0.art.get_root_key()?.key),
    );

    debug!("User 2 receive seccond changes ...");
    user2.art.update_private_art(&blank_user_1[1])?;
    debug!("User2 tk: {}", user2.art.get_root_key().unwrap().key);
    user2.epoch += 1;
    // debug!("art2:\n{}", user2.art.get_root());
    assert_eq!(user2.art.get_root(), user1.art.get_root());
    assert_eq!(
        user2.art.public_key_of(&user2.art.get_root_key()?.key),
        user1.art.public_key_of(&user1.art.get_root_key()?.key),
    );
    assert_eq!(user2.art.public_key_of(&user2.art.get_root_key()?.key), user1.art.get_root().public_key);

    debug!("User 2 update key ...");
    user2.update_key(None).await?;
    debug!("New TK.x: {}", user2.art.root.public_key.x);
    Ok(())
}

#[tokio::test]
async fn test_delete_group() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserIntegrationTestModel::new(DEFAULT_GROUP_SIZE, get_sender().await).await.0;

    context.delete_group().await?;
    Ok(())
}
