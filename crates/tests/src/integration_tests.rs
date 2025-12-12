use crate::{
    BACKEND_URL, CENTRIFUGO_URL, DEFAULT_NONCE_LENGTH, GROUP_SIZE, TEST_REPEATS,
    client_test_wrapper,
};
use crate::{user_test_model::UserTestModel, utils::*};
use ark_ec::{AffineRepr, CurveGroup};
use ark_std::UniformRand;
use ark_std::rand::prelude::StdRng;
use ark_std::rand::{Rng, SeedableRng};
use axum::http::StatusCode;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use bytes::{Bytes, BytesMut};
use cortado::{CortadoAffine, Fr};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use prost::Message;
use sha3::{Digest, Sha3_256};
use std::ops::Mul;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tracing::{Level, debug, debug_span, info, info_span, span, trace, warn, error_span, error};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::fmt::format;
use types::art_schemas::ProofMode;
use types::centrifugo_schemas::AuthRequest;
use types::protos::{Frame, FrameTbs, SpFrame, group_operation::Operation};
use zkp::rand::thread_rng;
use zrt_art::art::{AggregationContext, ArtAdvancedOps, PrivateArt, PublicArt};
use zrt_art::art_node::TreeMethods;
use zrt_art::changes::ApplicableChange;
use zrt_art::changes::branch_change::BranchChange;
use zrt_client_sdk::contexts::group::GroupContext;
use zrt_client_sdk::contexts::invite::InviteContext;
use zrt_client_sdk::models;
use zrt_crypto::schnorr::{sign, verify};

#[tokio::test]
async fn test_send_message() -> eyre::Result<()> {
    init_tracing_for_test();

    // let sender = get_integration_test_sender();

    let (context, init_message) = UserTestModel::new(GROUP_SIZE).await;
    info!("chat_uuid: {:?}", context.chat_uuid);

    let challenge = context.get_challenge().await?;

    let nonce = std::iter::repeat(rand::random::<u8>())
        .take(DEFAULT_NONCE_LENGTH as usize)
        .collect::<Vec<u8>>();

    let tk = context.art.root_secret_key();
    let pk = context.art.root_public_key();

    let signature = sign(&vec![tk], &vec![pk], &*Sha3_256::digest(&challenge)).unwrap();

    // Get Centrifugo auth token
    let auth_request = AuthRequest {
        chat_ids: vec![context.chat_uuid],
        proof: signature,
        nonce,
        challenge,
        epochs: vec![0],
    };

    let centrifugo_token_response = context.get_centrifugo_token(auth_request).await?;

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

    let tbs_frame = FrameTbs {
        group_id: context.chat_uuid.to_string(),
        epoch: 0,
        nonce: (0..DEFAULT_NONCE_LENGTH)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<u8>>(),
        group_operation: None,
        protected_payload: "zk messenger is the best".as_bytes().to_vec(),
    };

    let msg = Sha3_256::digest(tbs_frame.encode_to_vec()).to_vec();
    let tk = context.art.root_secret_key();
    let pk = vec![context.art.root_public_key()];

    let signature = sign(&vec![tk], &pk, &msg)?;
    let verification_result = verify(&signature, &pk, &msg);
    assert!(verification_result.is_ok());

    let req = Frame {
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
                            info!(
                                "send_message: Connection established, continue waiting for messages"
                            );
                            // info!("send_message: _connect_msg: {:#?}", _connect_msg.connect);
                        }
                        CentrifugoEvent::ChannelMessage(channel_msg) => {
                            let sp_frame = SpFrame::decode(&*channel_msg.publication.data).unwrap();
                            info!("sp_frame.frame: {:?}", sp_frame.frame);

                            let content_string = sp_frame.frame.unwrap().encode_to_vec();

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

    context.send_frame(req).await?;

    let result = tokio::time::timeout(Duration::from_secs(5), handle).await?;

    if let Err(e) = result {
        panic!("Error: {:?}", e);
    }

    Ok(())
}

#[tokio::test]
async fn test_get_message() -> eyre::Result<()> {
    init_tracing_for_test();
    let mut rng = StdRng::seed_from_u64(42);

    let (mut context, _) = UserTestModel::new(GROUP_SIZE).await;
    let test_context = context.derive_new(2)?;

    let mut messages = Vec::with_capacity(TEST_REPEATS + 1);
    // messages.push(init_message);
    for _ in 0..TEST_REPEATS {
        let (add_member_response, message) = context
            .add_member(Fr::rand(&mut rng), Some(StatusCode::OK))
            .await?;
        messages.push(message);
        assert_eq!(add_member_response.status(), StatusCode::OK);
    }

    let sp_frames = test_context.get_messages(TEST_REPEATS as i64, 1).await?;

    for (sp_frame, message) in sp_frames.sp_frames.iter().zip(messages.iter()) {
        let frame_message = Frame::decode(&**message)?;
        assert_eq!(frame_message, sp_frame.frame.clone().unwrap());
    }

    Ok(())
}

#[tokio::test]
async fn test_init_group() -> eyre::Result<()> {
    init_tracing_for_test();

    let _ = UserTestModel::new(GROUP_SIZE).await.0;

    Ok(())
}

#[tokio::test]
async fn test_add_member() -> eyre::Result<()> {
    init_tracing_for_test();
    let mut rng = StdRng::seed_from_u64(42);

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;
    for i in 0..TEST_REPEATS {
        info!("running {i}-th add member");
        context
            .add_member(Fr::rand(&mut rng), Some(StatusCode::OK))
            .await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_add_member_after_removal() -> eyre::Result<()> {
    init_tracing_for_test();
    let mut rng = StdRng::seed_from_u64(42);

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;

    for i in 0..TEST_REPEATS {
        info!(
            "running {i}-th iteration of make_blank(user_{}) + add member()",
            i + 1
        );
        let path = context.index_of(i + 1).unwrap().get_path().unwrap();
        context
            .make_blank(&path, Some(StatusCode::NO_CONTENT))
            .await?;
        debug!("Adding member...");
        context
            .add_member(Fr::rand(&mut rng), Some(StatusCode::OK))
            .await?;
    }

    Ok(())
}

/// Six users try to update the same epoch at the same time.
// TODO: fix test: transactions are run for the whole send_frame handling.
#[tokio::test]
async fn test_concurrent_art_update() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;

    info!("{:?}", context.chat_uuid);

    let mut user1 = context.derive_new(1).unwrap();
    let mut user2 = context.derive_new(2).unwrap();
    let mut user3 = context.derive_new(3).unwrap();
    let mut user4 = context.derive_new(4).unwrap();
    let mut user5 = context.derive_new(5).unwrap();
    let mut user6 = context.derive_new(6).unwrap();

    async fn send_frame(frame: Frame, url: String) -> bool {
        let mut buf = BytesMut::new();
        frame.encode(&mut buf).unwrap();
        let client = reqwest::Client::new();

        let response = client
            .post(url)
            .body(Bytes::from(buf.clone()))
            .send()
            .await
            .unwrap();

        matches!(response.status(), StatusCode::OK)
    }

    let frame1 = user1.create_key_update_frame(None).await.unwrap().0;
    let frame2 = user2.create_key_update_frame(None).await.unwrap().0;
    let frame3 = user3.create_key_update_frame(None).await.unwrap().0;
    let frame4 = user4.create_key_update_frame(None).await.unwrap().0;
    let frame5 = user5.create_key_update_frame(None).await.unwrap().0;
    let frame6 = user6.create_key_update_frame(None).await.unwrap().0;

    let url = format!(
        "{}/{}/{}/{}",
        BACKEND_URL, "v1/group", user1.chat_uuid, "frames"
    );

    let url1 = url.clone();
    let url2 = url.clone();
    let url3 = url.clone();
    let url4 = url.clone();
    let url5 = url.clone();
    let url6 = url.clone();

    let handle1 = tokio::spawn(async { send_frame(frame1, url1).await });
    let handle2 = tokio::spawn(async { send_frame(frame2, url2).await });
    let handle3 = tokio::spawn(async { send_frame(frame3, url3).await });
    let handle4 = tokio::spawn(async { send_frame(frame4, url4).await });
    let handle5 = tokio::spawn(async { send_frame(frame5, url5).await });
    let handle6 = tokio::spawn(async { send_frame(frame6, url6).await });

    let mut ok_count = 0;
    ok_count += handle1.await.unwrap() as i64;
    ok_count += handle2.await.unwrap() as i64;
    ok_count += handle3.await.unwrap() as i64;
    ok_count += handle4.await.unwrap() as i64;
    ok_count += handle5.await.unwrap() as i64;
    ok_count += handle6.await.unwrap() as i64;

    #[cfg(feature = "merge_changes")]
    let result_count = 6;
    #[cfg(not(feature = "merge_changes"))]
    let result_count = 1;

    assert_eq!(
        ok_count, result_count,
        "Failed to prevent the merge, when merge_changes is disabled."
    );

    Ok(())
}

#[tokio::test]
async fn test_remove_member() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;
    let mut retrieval_context = context.derive_new(1)?;

    // skip the root node
    for i in 0..TEST_REPEATS {
        let path = context.index_of(i + 5).unwrap().get_path().unwrap();
        context
            .make_blank(&path, Some(StatusCode::NO_CONTENT))
            .await?;

        retrieval_context.art.commit().unwrap();
        let received_art = retrieval_context
            .get_art((i + 1) as u64, None, ProofMode::UseLeafKey.to_string())
            .await?;

        assert_eq!(
            received_art.root().data().weight(),
            retrieval_context.art.root().data().weight() - 1
        );

        retrieval_context.art = PrivateArt::new(received_art, context.art.leaf_secret_key())?;

        let sk_to_use = retrieval_context.art.secrets().root();
        let received_art_check = retrieval_context
            .get_art(
                (i + 1) as u64,
                Some(sk_to_use),
                ProofMode::UseRootKey.to_string(),
            )
            .await?;

        assert_eq!(received_art_check.root(), retrieval_context.art.root());
    }

    Ok(())
}

#[tokio::test]
async fn test_get_art() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;
    let mut retrieval_context = context.derive_new(2)?;
    let mut art_roots = vec![context.art.root().data().public_key()];

    // update art several times, so we can retrieve them
    for _ in 0..TEST_REPEATS {
        context.update_key(None, Some(StatusCode::OK)).await?;
        let root_sk = context.art.secrets().preview().root();
        let root_pk = CortadoAffine::generator().mul(root_sk).into_affine();
        art_roots.push(root_pk);
    }

    // Test if retrieval is correct
    for i in 0..TEST_REPEATS {
        let received_art = retrieval_context
            .get_art(i as u64, None, ProofMode::UseLeafKey.to_string())
            .await?;

        assert_eq!(received_art.root().data().public_key(), art_roots[i]);

        retrieval_context.art =
            PrivateArt::new(received_art, retrieval_context.initial_secrets[1])?;
    }

    Ok(())
}

#[cfg(feature = "merge_changes")]
#[tokio::test]
async fn test_epoch_merge() -> eyre::Result<()> {
    init_tracing_for_test();
    let mut rng = StdRng::seed_from_u64(42);

    let payload = UserTestModel::new_nonce();

    let (mut user0, _) = UserTestModel::new(GROUP_SIZE).await;
    let mut user1 = user0.derive_new(1)?;
    let mut user2 = user0.derive_new(2)?;
    let mut user3 = user0.derive_new(5)?;

    // sanity check
    assert_eq!(user2.art.root(), user0.art.root());
    assert_eq!(user1.art.root(), user0.art.root());
    assert_eq!(user3.art.root(), user0.art.root());

    info!("User 1 update key ...");
    user1
        .update_key(Some(payload.clone()), Some(StatusCode::OK))
        .await?;
    info!("User1 TK: {}", user1.art.root().data().public_key());

    info!("User 3 add member ...");
    user3
        .update_key(Some(payload.clone()), Some(StatusCode::OK))
        .await?;
    info!("User3 TK: {}", user3.art.root().data().public_key());

    let changes = user2
        .get_changes(20, 0, None, Some(StatusCode::ACCEPTED))
        .await?;

    for change in changes {
        change.apply(&mut user2.art)?;
        change.apply(&mut user0.art)?;
    }

    info!("User 2 merge changes locally ...");

    // user2.art.merge_for_observer(&changes);
    // user0.art.merge_for_observer(&changes);
    user2.epoch += 1;
    user0.epoch += 1;
    info!("User2 MTK_x: {}", user2.art.root().data().public_key());

    info!("User 2 update and send update request with merge resolved");
    user2
        .update_key(Some(payload.clone()), Some(StatusCode::OK))
        .await?;
    info!("User2 TK_x: {}", user2.art.root().data().public_key());

    info!("User 0 fail to append member ...");
    user0
        .add_member(Fr::rand(&mut rng), Some(StatusCode::UNAUTHORIZED))
        .await?;
    // user0.update_key(None, Some(StatusCode::OK)).await?;
    info!("User0 TK: {}", user0.art.root().data().public_key());

    Ok(())
}

#[cfg(feature = "merge_changes")]
#[tokio::test]
async fn test_merge_operations_after_removal() -> eyre::Result<()> {
    init_tracing_for_test();
    let mut rng = StdRng::seed_from_u64(42);

    let payload = UserTestModel::new_nonce();

    let (mut user0, _) = UserTestModel::new(GROUP_SIZE).await;
    let mut user1 = user0.derive_new(1)?;
    let mut user2 = user0.derive_new(2)?;
    let mut user3 = user0.derive_new(5)?;

    // sanity check
    assert_eq!(user2.art.root(), user0.art.root());
    assert_eq!(user1.art.root(), user0.art.root());
    assert_eq!(user3.art.root(), user0.art.root());

    let target_user_index = user1.art.node_index();
    let target_user_path = target_user_index.get_path().unwrap();

    info!("User 0 remove user 1");
    user0
        .make_blank(&target_user_path, Some(StatusCode::NO_CONTENT))
        .await?;

    info!("User 1 fails to updates his key");
    let mut user1_clone = user1.clone();
    user1_clone
        .update_key(Some(payload.clone()), Some(StatusCode::UNAUTHORIZED))
        .await?;

    info!("User 1 fails to add member");
    let mut user1_clone = user1.clone();
    user1_clone
        .add_member(Fr::rand(&mut rng), Some(StatusCode::UNAUTHORIZED))
        .await?;

    info!("User 1 fails to leave the group as he already removed.");
    let mut user1_clone = user1.clone();
    user1_clone
        .leave_group(Some(StatusCode::UNAUTHORIZED))
        .await?;

    info!("User 1 fails to send message, as he is in previous epoch.");
    let mut user1_clone = user1.clone();
    user1_clone
        .send_payload(
            b"User 1 still can send message.".to_vec(),
            Some(StatusCode::UNAUTHORIZED),
        )
        .await?;

    info!("User 0 can send messages.");
    user0
        .send_payload(
            b"User 1 still can send message.".to_vec(),
            Some(StatusCode::OK),
        )
        .await?;

    Ok(())
}

#[tokio::test]
async fn test_merge_for_removal() -> eyre::Result<()> {
    init_tracing_for_test();

    let (mut user0, _) = UserTestModel::new(7).await;
    let mut user1 = user0.derive_new(5)?;
    let mut user2 = user0.derive_new(2)?;
    let user3 = user0.derive_new(1)?;

    // sanity check
    assert_eq!(user2.art, user0.art);
    assert_eq!(user1.art, user0.art);
    assert_eq!(user3.art, user0.art);

    let target_node_path = user0.index_of(4).unwrap().get_path().unwrap();

    info!(
        "User 0 removing the user on path {:?} ...",
        &target_node_path
    );
    user0
        .make_blank(&target_node_path, Some(StatusCode::NO_CONTENT))
        .await?;
    user0.art.commit().unwrap();

    info!("User1 receive changes ..");
    let blank_user_0 = user1
        .get_changes(20, 0, None, Some(StatusCode::ACCEPTED))
        .await?;

    assert_eq!(user1.art, user2.art);
    blank_user_0[0].apply(&mut user1.art).unwrap();
    user1.art.commit().unwrap();
    assert_eq!(user1.art, user0.art);
    user1.epoch += 1;

    info!("User 1 remove the target node ...");
    user1
        .make_blank(&target_node_path, Some(StatusCode::NO_CONTENT))
        .await?;
    user1.art.commit().unwrap();
    assert_eq!(
        CortadoAffine::generator()
            .mul(user1.art.root_secret_key())
            .into_affine(),
        user1.art.root().data().public_key()
    );

    info!("User 2 receive changes ...");
    let blank_user_1 = user2
        .get_changes(20, 0, None, Some(StatusCode::ACCEPTED))
        .await?;
    blank_user_1[0].apply(&mut user2.art).unwrap();
    user2.art.commit().unwrap();
    user2.epoch += 1;
    assert_eq!(user2.art, user0.art);
    assert_eq!(
        CortadoAffine::generator()
            .mul(user2.art.root_secret_key())
            .into_affine(),
        CortadoAffine::generator()
            .mul(user0.art.root_secret_key())
            .into_affine(),
    );

    info!("User 2 receive second changes ...");
    blank_user_1[1].apply(&mut user2.art)?;
    user2.art.commit().unwrap();
    user2.epoch += 1;
    assert_eq!(
        user1.art.root(),
        user2.art.root(),
        "Users have different view on the state of the art.\nUser1\n{}\nUser2\n{}",
        user1.art.root(),
        user2.art.root(),
    );
    assert_eq!(
        CortadoAffine::generator()
            .mul(user2.art.root_secret_key())
            .into_affine(),
        CortadoAffine::generator()
            .mul(user1.art.root_secret_key())
            .into_affine(),
    );
    assert_eq!(
        CortadoAffine::generator()
            .mul(user2.art.root_secret_key())
            .into_affine(),
        user1.art.root().data().public_key()
    );

    info!("User 2 update key ...");
    user2.update_key(None, Some(StatusCode::OK)).await?;

    Ok(())
}

#[tokio::test]
async fn test_leave() -> eyre::Result<()> {
    init_tracing_for_test();

    let (mut user0, _) = UserTestModel::new(7).await;
    let mut user1 = user0.derive_new(5)?;
    let mut user2 = user0.derive_new(2)?;
    let mut user3 = user0.derive_new(1)?;
    info!(
        "User0 pk: {}",
        CortadoAffine::generator()
            .mul(user0.art.leaf_secret_key())
            .into_affine()
    );

    // let target_node_index = user0.art.node_index().clone();
    let target_node_path = user0.art.node_index().get_path().unwrap();

    // sanity check
    assert_eq!(user1.art, user0.art);
    assert_eq!(user2.art, user0.art);
    assert_eq!(user3.art, user0.art);

    info!("User 0 leave the group ...");
    user0.leave_group(Some(StatusCode::OK)).await?;
    user0.epoch -= 1;

    info!("User 0 Fails to leave the second time ...");
    let mut tmp = user0.clone();
    tmp.leave_group(Some(StatusCode::UNAUTHORIZED)).await?;

    info!("User 0 fail to update key, as he is removed ...");
    let mut tmp = user0.clone();
    tmp.update_key(None, Some(StatusCode::UNAUTHORIZED)).await?;

    info!("User1 receive changes ..");
    let mut blank_user_0 = user1
        .get_frames(20, 0, None, Some(StatusCode::ACCEPTED))
        .await?
        .sp_frames;
    // let key_update = extract_branch_changes(&blank_user_0.pop().unwrap().frame.unwrap()).unwrap().unwrap();
    let leave_operation = UserTestModel::unwrap_operation(blank_user_0.pop().unwrap());
    assert!(matches!(leave_operation, Operation::LeaveGroup(_)));

    let leave_change = if let Operation::LeaveGroup(bytes) = leave_operation {
        Some(postcard::from_bytes::<BranchChange<CortadoAffine>>(&bytes)?)
    } else {
        None
    }
    .unwrap();

    user1.art.commit()?;
    leave_change.apply(&mut user1.art)?;
    user1.epoch += 1;

    info!("User 1 blanks the target node ...");
    user1
        .make_blank(&target_node_path, Some(StatusCode::NO_CONTENT))
        .await?;

    info!("User 2 receive changes ...");
    let changes = user2
        .get_changes(20, 0, None, Some(StatusCode::ACCEPTED))
        .await?;

    info!("Changes received: {:?}", changes);

    for change in &changes {
        change.apply(&mut user2.art).unwrap();
        user2.art.commit().unwrap();
        user2.epoch += 1;
    }

    user1.art.commit().unwrap();

    // info!("art2:\n{}", user2.art.root());
    assert_eq!(
        user2.art,
        user1.art,
        "Users have different wiew on the state of the art:\nuser 2:\n{}\nUser1\n{}",
        user2.art.root(),
        user1.art.root(),
    );
    assert_eq!(
        CortadoAffine::generator()
            .mul(user2.art.root_secret_key())
            .into_affine(),
        CortadoAffine::generator()
            .mul(user1.art.root_secret_key())
            .into_affine(),
    );

    info!("User 2 blank the target node ...");
    user2
        .make_blank(&target_node_path, Some(StatusCode::NO_CONTENT))
        .await?;
    Ok(())
}

#[tokio::test]
async fn test_leave_after_removal() -> eyre::Result<()> {
    init_tracing_for_test();

    let (mut user0, _) = UserTestModel::new(7).await;
    let mut user1 = user0.derive_new(5)?;
    let mut user3 = user0.derive_new(1)?;
    let target_node_path = user3.art.node_index().get_path().unwrap();

    info!("User 0 blanks the target node ...");
    user0
        .make_blank(&target_node_path, Some(StatusCode::NO_CONTENT))
        .await?;

    info!("User1 receive changes ..");
    let blank_user_3_0 = user1
        .get_changes(20, 0, None, Some(StatusCode::ACCEPTED))
        .await?;
    assert_eq!(blank_user_3_0.len(), 1);

    info!("User 3 fails to leave the group ...");
    user3.leave_group(Some(StatusCode::UNAUTHORIZED)).await?;

    blank_user_3_0[0].apply(&mut user1.art).unwrap();
    user1.epoch += 1;

    info!("User 2 blank the target node ...");
    user1
        .make_blank(&target_node_path, Some(StatusCode::NO_CONTENT))
        .await?;
    Ok(())
}

#[tokio::test]
async fn test_delete_group() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;

    context.delete_group().await?;
    Ok(())
}

#[tokio::test]
async fn test_send_aggregated_change() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut rng = StdRng::seed_from_u64(42);
    let (mut context, _) = UserTestModel::new(7).await;

    let zero_art = context.art.clone();
    info!("zero_art leaf pk: {}", zero_art.leaf_public_key());

    let mut agg = AggregationContext::from(zero_art.clone());

    for _ in 0..8 {
        agg.add_member(Fr::rand(&mut rng))?;
    }

    context
        .send_aggregation(&agg, None, None, Some(StatusCode::OK))
        .await?;

    for _ in 0..3 {
        debug!("context art:\n{}", context.art.root());
        context.update_key(None, Some(StatusCode::OK)).await?;
        context
            .add_member(Fr::rand(&mut rng), Some(StatusCode::OK))
            .await?;
    }

    for _ in 0..3 {
        context.update_key(None, Some(StatusCode::OK)).await?;
    }

    Ok(())
}

#[tokio::test]
/// Add member is a unique operation, so the following key update must fail.
async fn test_epoch_validity_check() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut rng = StdRng::seed_from_u64(42);

    let (mut user0, _) = UserTestModel::new(7).await;
    let mut user1 = user0.derive_new(1)?;

    user0
        .add_member(Fr::rand(&mut rng), Some(StatusCode::OK))
        .await
        .unwrap();
    user1
        .update_key(
            Some(b"askjdfhlaklsd".to_vec()),
            Some(StatusCode::UNAUTHORIZED),
        )
        .await
        .unwrap();

    Ok(())
}

#[tokio::test]
async fn test_polling() -> eyre::Result<()> {
    init_tracing_for_test();
    let mut rng = StdRng::seed_from_u64(42);

    let (mut user0, _) = UserTestModel::new(7).await;
    let mut user1 = user0.derive_new(1)?;
    let mut user2 = user0.derive_new(2)?;
    let mut user3 = user0.derive_new(3)?;

    info!("user0 adds member, while user1 polls data ...");
    for _ in 0..5 {
        user0
            .add_member(Fr::rand(&mut rng), Some(StatusCode::OK))
            .await
            .unwrap();
        user1.poll(None).await.unwrap();
    }

    info!("user2 polls all the available data ...");
    user2.poll(None).await.unwrap();

    // Useless pols
    for _ in 0..4 {
        user2.poll(None).await.unwrap();
    }

    info!("two users updates their keys, and user 2 polls changes ...");
    for _ in 0..6 {
        while user1
            .update_key(Some(UserTestModel::new_nonce()), Some(StatusCode::OK))
            .await
            .is_err()
        {
            user1.poll(None).await.unwrap();
        }
        while user0
            .update_key(Some(UserTestModel::new_nonce()), Some(StatusCode::OK))
            .await
            .is_err()
        {
            user0.poll(None).await.unwrap();
        }

        user0.poll(None).await.unwrap();
        user1.poll(None).await.unwrap();
        user2.poll(None).await.unwrap();
    }
    let mut commited_art = user0.art.clone();
    commited_art.commit().unwrap();
    let mut agg = AggregationContext::from(commited_art);

    let mut rng = StdRng::seed_from_u64(rand::random());
    for _ in 0..24 {
        agg.add_member(Fr::rand(&mut rng))?;
    }
    let new_key = Fr::rand(&mut rng);
    agg.update_key(new_key)?;

    user0
        .send_aggregation(&agg, Some(new_key), None, Some(StatusCode::OK))
        .await?;

    info!("user1 and user2 polls aggregations polls all the available data ...");
    user1.poll(None).await.unwrap();
    user2.poll(None).await.unwrap();

    info!("user3 polls all the available data ...");
    user3.poll(None).await.unwrap();
    user3.poll(None).await.unwrap();

    user3.update_key(None, Some(StatusCode::OK)).await.unwrap();
    user0.update_key(None, Some(StatusCode::OK)).await.unwrap();
    user2.update_key(None, Some(StatusCode::OK)).await.unwrap();

    user0.poll(None).await.unwrap();
    user0
        .add_member(Fr::rand(&mut rng), Some(StatusCode::OK))
        .await
        .unwrap();

    Ok(())
}

use crate::client_test_wrapper::InviteClientWrapper;
use client_test_wrapper::ClientWrapper;
use types::messenger_schemas::GetMessageQuery;

/// Test Flow:
/// - Create epoch with one user
/// - Join with the second user
/// - Cyclic key update with two users
#[cfg(feature = "merge_changes")]
#[tokio::test]
async fn test_flow() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut rng = StdRng::seed_from_u64(42);

    let (mut client0, frame0) = ClientWrapper::new_group(&mut rng);
    let response = ClientWrapper::send_frame(frame0.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::CREATED),
        "expected StatusCode::CREATED, while got {}",
        response.0.status()
    );
    client0.process_frame(frame0).unwrap();

    let sk = Fr::rand(&mut rng);
    let (frame, invite) = client0.add_member(sk)?;
    let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::OK),
        "expected StatusCode::OK, while got {}",
        response.0.status()
    );

    info!("Join group with client1...");
    let client1 = InviteClientWrapper::new(sk, invite);
    let mut client1 = client1.apply_join_frame().await.unwrap();
    client1.process_frame(frame.clone()).unwrap();
    client0.process_frame(frame).unwrap();

    let frame = client1.join_group().unwrap();
    let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::OK),
        "expected StatusCode::OK, while got {}",
        response.0.status()
    );
    client1.process_frame(frame.clone()).unwrap();
    client0.process_frame(frame).unwrap();

    info!("Create new frame with client1");
    let frame = client1.create_frame(b"some data".to_vec()).unwrap();
    debug!("frame: {frame:?}");
    let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::OK),
        "expected StatusCode::OK, while got {}",
        response.0.status()
    );
    client1.process_frame(frame.clone()).unwrap();
    client0.process_frame(frame).unwrap();

    for _ in 0..10 {
        info!("Create new frame with client0");
        let frame = client0.create_frame(b"some other data".to_vec()).unwrap();
        let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
        assert!(
            matches!(response.0.status(), StatusCode::OK),
            "expected StatusCode::OK, while got {}",
            response.0.status()
        );
        client1.process_frame(frame.clone()).unwrap();
        client0.process_frame(frame).unwrap();

        info!("Create new frame with client1");
        let frame = client1.create_frame(b"some data".to_vec()).unwrap();
        let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
        assert!(
            matches!(response.0.status(), StatusCode::OK),
            "expected StatusCode::OK, while got {}",
            response.0.status()
        );
        client1.process_frame(frame.clone()).unwrap();
        client0.process_frame(frame).unwrap();
    }

    Ok(())
}

/// Test Flow:
/// - Create epoch with one user
/// - Join with the second user
/// - Cyclic key update with two users on the same epoch
#[cfg(feature = "merge_changes")]
#[tokio::test]
async fn test_flow_with_merge() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut rng = StdRng::seed_from_u64(42);

    let (mut client0, frame0) = ClientWrapper::new_group(&mut rng);
    let response = ClientWrapper::send_frame(frame0.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::CREATED),
        "expected StatusCode::CREATED, while got {}",
        response.0.status()
    );
    client0.process_frame(frame0).unwrap();

    info!("Add first member...");
    let sk = Fr::rand(&mut rng);
    let (frame, invite) = client0.add_member(sk)?;
    let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::OK),
        "expected StatusCode::OK, while got {}",
        response.0.status()
    );

    info!("Join group with first member...");
    let client1 = InviteClientWrapper::new(sk, invite);
    let mut client1 = client1.apply_join_frame().await.unwrap();
    client1.process_frame(frame.clone()).unwrap();
    client0.process_frame(frame).unwrap();

    let frame = client1.join_group().unwrap();
    let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::OK),
        "expected StatusCode::OK, while got {}",
        response.0.status()
    );
    client1.process_frame(frame.clone()).unwrap();
    client0.process_frame(frame).unwrap();

    info!("Add second member...");
    let sk = Fr::rand(&mut rng);
    let (frame, invite) = client0.add_member(sk)?;
    let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::OK),
        "expected StatusCode::OK, while got {}",
        response.0.status()
    );

    info!("Join group with second member...");
    let client2 = InviteClientWrapper::new(sk, invite);
    let mut client2 = client2.apply_join_frame().await.unwrap();
    client2.process_frame(frame.clone()).unwrap();
    client1.process_frame(frame.clone()).unwrap();
    client0.process_frame(frame).unwrap();

    let frame = client2.join_group().unwrap();
    let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::OK),
        "expected StatusCode::OK, while got {}",
        response.0.status()
    );
    client2.process_frame(frame.clone()).unwrap();
    client1.process_frame(frame.clone()).unwrap();
    client0.process_frame(frame).unwrap();

    for i in 0..10 {
        info!("Create new frame with both users client0. {i}-th iteration");
        let frame0 = client0.create_frame(b"some other data".to_vec()).unwrap();
        let frame1 = client1.create_frame(b"some data".to_vec()).unwrap();

        let response0 = ClientWrapper::send_frame(frame0.clone()).await.unwrap();
        let response1 = ClientWrapper::send_frame(frame1.clone()).await.unwrap();

        assert!(
            matches!(response.0.status(), StatusCode::OK),
            "expected StatusCode::OK, while got {}",
            response0.0.status()
        );
        assert!(
            matches!(response.0.status(), StatusCode::OK),
            "expected StatusCode::OK, while got {}",
            response1.0.status()
        );

        client2.process_frame(frame0.clone()).unwrap();
        client1.process_frame(frame0.clone()).unwrap();
        client0.process_frame(frame0).unwrap();

        client2.process_frame(frame1.clone()).unwrap();
        client1.process_frame(frame1.clone()).unwrap();
        client0.process_frame(frame1).unwrap();

        info!("send frame with second user for correctness");
        let frame = client2.create_frame(b"data for client 2".to_vec()).unwrap();
        let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
        assert!(
            matches!(response.0.status(), StatusCode::OK),
            "expected StatusCode::OK, while got {}",
            response0.0.status()
        );

        client2.process_frame(frame.clone()).unwrap();
        client1.process_frame(frame.clone()).unwrap();
        client0.process_frame(frame).unwrap();
    }

    Ok(())
}

/// Test Flow:
/// - Create epoch with one user
/// - Join with the second user
/// - Cyclic key update with two users
#[cfg(feature = "merge_changes")]
#[tokio::test]
async fn test_flow_send_frame() -> eyre::Result<()> {
    init_tracing_for_test();
    let seed = 42;

    let err_span = error_span!(
        "test_send_frame",
        seed = ?seed,
    );
    let _ = err_span.enter();

    let mut rng = StdRng::seed_from_u64(seed);

    let (mut client0, frame0) = ClientWrapper::new_group(&mut rng);
    let response = ClientWrapper::send_frame(frame0.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::CREATED),
        "expected StatusCode::CREATED, while got {}",
        response.0.status()
    );
    client0.process_frame(frame0).unwrap();

    let sk = Fr::rand(&mut rng);
    let (frame, invite) = client0.add_member(sk)?;
    let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::OK),
        "expected StatusCode::OK, while got {}",
        response.0.status()
    );

    info!("Join group...");
    let client1 = InviteClientWrapper::new(sk, invite);
    let mut client1 = client1.apply_join_frame().await.unwrap();
    client1.process_frame(frame.clone()).unwrap();
    client0.process_frame(frame).unwrap();

    let frame = client1.join_group().unwrap();
    let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::OK),
        "expected StatusCode::OK, while got {}",
        response.0.status()
    );

    debug!("client0 epoch: {}", client0.group_context().epoch());
    debug!("client1 epoch: {}", client1.group_context().epoch());

    async fn try_send(mut client_wrapper: ClientWrapper, client_name: &str) {
        let logger = logger_for_test(&format!("test_flow_send_frame-{}.dev.log", client_name));
        let _thread_logger_guard = tracing::subscriber::set_default(logger);

        // let span = info_span!(parent: None, "try_send", client_name);
        // let _enter = span.enter();

        info!("Pre poll messages...");
        client_wrapper.poll().await.unwrap();
        info!("Start sending messages...");

        for i in 0..20 {
            loop {
                let frame = client_wrapper.create_frame(b"some data".to_vec()).unwrap();
                let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();

                if matches!(response.0.status(), StatusCode::OK) {
                    info!("Frame send. poll and wait to send more");
                    client_wrapper.poll().await.unwrap();
                    thread::sleep(Duration::from_millis(20));
                    break;
                } else {
                    error!("Failed to send message, try to poll before retry");
                    thread::sleep(Duration::from_millis(10));
                    client_wrapper.poll().await.unwrap();
                }
            }
        }
    }

    info!("Run concurrent updates...");

    let handle0 = tokio::spawn(async { try_send(client0, "client0").await });
    let handle1 = tokio::spawn(async { try_send(client1, "client1").await });

    handle0.await.unwrap();
    handle1.await.unwrap();

    info!("Run finished successfully");

    Ok(())
}


/// Test Flow:
/// - Create epoch with one user
/// - Join with the second user
/// - Cyclic key update with two users
#[cfg(feature = "merge_changes")]
// #[tokio::test]
async fn test_flow_send_frame_in_bunch() -> eyre::Result<()> {
    init_tracing_for_test();
    let seed = 42;

    let mut rng = StdRng::seed_from_u64(seed);

    let (mut client0, frame0) = ClientWrapper::new_group(&mut rng);
    let response = ClientWrapper::send_frame(frame0.clone()).await.unwrap();
    assert!(
        matches!(response.0.status(), StatusCode::CREATED),
        "expected StatusCode::CREATED, while got {}",
        response.0.status()
    );
    client0.poll().await.unwrap();

    info!("add other user to the group...");
    let members_secrets: Vec<Fr> = (0..10)
        .into_iter()
        .map(|_| Fr::rand(&mut rng))
        .collect();

    let mut contexts = Vec::with_capacity(members_secrets.len());

    for secret_key in members_secrets.iter() {
        client0.poll().await.unwrap();
        let (frame, invite) = client0.add_member(*secret_key)?;
        let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
        assert!(
            matches!(response.0.status(), StatusCode::OK),
            "expected StatusCode::OK, while got {}",
            response.0.status()
        );

        let member = InviteClientWrapper::new(*secret_key, invite);
        let mut member = member.apply_join_frame().await.unwrap();
        member.process_frame(frame.clone()).unwrap();

        let frame = member
            .join_group()
            .expect("Failed to join group");

        let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();
        assert!(
            matches!(response.0.status(), StatusCode::OK),
            "expected StatusCode::OK, while got {}",
            response.0.status()
        );

        contexts.push(member);
    }

    contexts.insert(0, client0);

    async fn try_send(mut client_wrapper: ClientWrapper, client_name: String) {
        info!("Pre poll messages...");
        client_wrapper.poll().await.unwrap();

        info!("Start sending messages...");
        for i in 0..8 {
            loop {
                let frame = client_wrapper.create_frame(b"some data".to_vec()).unwrap();
                let response = ClientWrapper::send_frame(frame.clone()).await.unwrap();

                if matches!(response.0.status(), StatusCode::OK) {
                    info!("Frame send. poll and wait to send more");
                    client_wrapper.poll().await.unwrap();
                    thread::sleep(Duration::from_millis(20));
                    break;
                } else {
                    warn!("Failed to send message, try to poll before retry");
                    thread::sleep(Duration::from_millis(10));
                    client_wrapper.poll().await.unwrap();
                }
            }
        }
    }

    info!("Run concurrent updates...");

    let mut handles = Vec::with_capacity(contexts.len());
    for (i, client) in contexts.into_iter().enumerate() {
        let client_name = format!("client{i}");
        let logger = logger_for_test(&format!("test_flow_send_frame_in_bunch-{}.dev.log", client_name));

        let span = info_span!(parent: None, "try_send", client_name);
        let _enter = span.enter();

        handles.push(tokio::spawn(async move {
            try_send(client, client_name)
                .with_subscriber(logger)
                .await
        }));
    }

    for handle in handles.into_iter().rev() {
        handle.await.unwrap();
    }

    info!("Run finished successfully");

    Ok(())
}
