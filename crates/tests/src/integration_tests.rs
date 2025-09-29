use std::time::Duration;
use crate::{
    user_test_model::UserTestModel,
    utils::*,
};
use types::centrifugo_schemas::AuthRequest;
use ark_std::rand::SeedableRng;
use ark_std::rand::prelude::StdRng;
use art::traits::{ARTPrivateAPI, ARTPrivateView, ARTPublicAPI, ARTPublicView};
use art::types::{PrivateART, PublicART};
use axum::http::StatusCode;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use bytes::{Bytes, BytesMut};
use cortado::CortadoAffine;
use crypto::schnorr::{sign, verify};
use eventsource_stream::Eventsource;
use prost::Message;
use tracing::debug;
use tracing::field::debug;
use types::art_schemas::{ChallengeResponse, GetARTResponse, ProofMode};
use types::protos;
use types::protos::{Frame, SpFrame, SpFrames, FrameTbs};
use sha3::{Sha3_256, Digest};
use futures::StreamExt;
use crate::{
    CENTRIFUGO_URL,
    TEST_REPEATS,
    DEFAULT_NONCE_LENGTH,
    GROUP_SIZE,
};

#[tokio::test]
async fn test_send_message() -> eyre::Result<()> {
    init_tracing_for_test();

    // let sender = get_integration_test_sender();

    let (mut context, init_message) = UserTestModel::new(GROUP_SIZE).await;
    debug!("chat_uuid: {:?}", context.chat_uuid);

    let challenge = context.get_challenge().await?;

    let nonce = std::iter::repeat(rand::random::<u8>())
        .take(DEFAULT_NONCE_LENGTH as usize)
        .collect::<Vec<u8>>();

    let tk = context.art.get_root_key().unwrap().key;
    let pk = context.art.get_root().public_key;

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
    let tk = context.art.get_root_key()?.key;
    let pk = vec![context.art.root.public_key];

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
                            debug!(
                                "send_message: Connection established, continue waiting for messages"
                            );
                            // debug!("send_message: _connect_msg: {:#?}", _connect_msg.connect);
                        }
                        CentrifugoEvent::ChannelMessage(channel_msg) => {
                            let sp_frame = SpFrame::decode(&*channel_msg.publication.data).unwrap();
                            debug!("sp_frame.frame: {:?}", sp_frame.frame);

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

    context
        .send_frame(req)
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

    let (mut context, init_message) = UserTestModel::new(GROUP_SIZE).await;
    let mut test_context = context.derive_new(2)?;

    let mut messages = Vec::with_capacity(TEST_REPEATS + 1);
    // messages.push(init_message);
    for _ in 0..TEST_REPEATS {
        let (add_member_response, message) = context.add_member().await?;
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

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;

    Ok(())
}

#[tokio::test]
async fn test_add_member() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;
    for _ in 0..TEST_REPEATS {
        context.add_member().await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_add_member_after_removal() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;

    for i in 0..TEST_REPEATS {
        let path = context.index_of(i + 1).unwrap().get_path().unwrap();
        context.make_blank(&path).await?;
        context.add_member().await?;
    }

    Ok(())
}

/// Six users try to update the same epoch at the same time.
#[tokio::test]
async fn test_concurent_art_update() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;

    debug!("{:?}", context.chat_uuid);

    let mut user1 = context.derive_new(1).unwrap();
    let mut user2 = context.derive_new(2).unwrap();
    let mut user3 = context.derive_new(3).unwrap();
    let mut user4 = context.derive_new(4).unwrap();
    let mut user5 = context.derive_new(5).unwrap();
    let mut user6 = context.derive_new(6).unwrap();


    let handle1 = tokio::spawn(async move {
    user1.update_key(None).await.is_ok()
    });

    let handle2 = tokio::spawn(async move {
    user2.update_key(None).await.is_ok()
    });

    let handle3 = tokio::spawn(async move {
    user3.update_key(None).await.is_ok()
    });

    let handle4 = tokio::spawn(async move {
    user4.update_key(None).await.is_ok()
    });

    let handle5 = tokio::spawn(async move {
    user5.update_key(None).await.is_ok()
    });

    let handle6 = tokio::spawn(async move {
    user6.update_key(None).await.is_ok()
    });

    let mut  ok_count = 0;
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
        ok_count,
        result_count,
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

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;
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

#[cfg(feature = "merge_changes")]
#[tokio::test]
async fn test_epoch_merge() -> eyre::Result<()> {
    init_tracing_for_test();

    let payload = UserTestModel::new_nonce();

    let (mut user0, _) = UserTestModel::new(GROUP_SIZE).await;
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
    debug!("User0 TK: {}", user0.art.root.public_key);

    debug!("User 1 update key ...");
    user1.update_key(Some(payload.clone())).await?;
    debug!("User1 TK: {}", user1.art.root.public_key);

    debug!("User 3 add member ...");
    user3.update_key(Some(payload.clone())).await?;
    debug!("User3 TK: {}", user3.art.root.public_key);

    let changes = user2.get_changes(20, 0, None).await?;

    debug!("User 2 merge changes locally ...");
    user2
        .art
        .recompute_path_secrets_for_observer(&changes)
        .unwrap();
    user2.art.merge(&changes)?;
    user2.epoch += 1;
    debug!("User2 MTK_x: {}", user2.art.root.public_key);

    assert_eq!(
        user2
            .art
            .public_key_of(&user2.art.get_root_key().unwrap().key),
        user2.art.root.public_key,
        "Check if secret on path is the same one used for art root pub key computation."
    );

    debug!("User 2 update and send update request with merge resolved");
    user2.update_key(Some(payload.clone())).await?;
    debug!("User2 TK_x: {}", user2.art.root.public_key);

    Ok(())
}

#[tokio::test]
async fn test_merge_for_removal() -> eyre::Result<()> {
    init_tracing_for_test();

    let (mut user0, _) = UserTestModel::new(7).await;
    let mut user1 = user0.derive_new(5)?;
    let mut user2 = user0.derive_new(2)?;
    let user3 = user0.derive_new(1)?;
    debug!(
        "User0 pk: {}",
        user0.art.public_key_of(&user0.art.get_secret_key())
    );

    // sanity check
    assert_eq!(user2.art.get_root(), user0.art.get_root());
    assert_eq!(user1.art.get_root(), user0.art.get_root());
    assert_eq!(user3.art.get_root(), user0.art.get_root());

    let target_node_path = user0.index_of(4).unwrap().get_path().unwrap();

    debug!("User 0 update key ...");
    user0.make_blank(&target_node_path).await?;
    // user0.update_key(None).await?;
    debug!("User0 TK: {}", user0.art.root.public_key);
    debug!("User0 tk: {}", user0.art.get_root_key().unwrap().key);

    debug!("User1 receive changes ..");
    let blank_user_0 = user1.get_changes(20, 0, None).await?;
    user1.art.update_private_art(&blank_user_0[0]).unwrap();
    user1.epoch += 1;
    debug!("User1 tk: {}", user1.art.get_root_key().unwrap().key);
    assert_eq!(user1.art.get_root(), user0.art.get_root());

    debug!("User 1 update key ...");
    user1.make_blank(&target_node_path).await?;
    debug!("User1 TK: {}", user1.art.root.public_key);
    debug!("User1 tk: {}", user1.art.get_root_key().unwrap().key);
    assert_eq!(
        user1.art.public_key_of(&user1.art.get_root_key()?.key),
        user1.art.get_root().public_key
    );

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
    assert_eq!(
        user2.art.public_key_of(&user2.art.get_root_key()?.key),
        user1.art.get_root().public_key
    );

    debug!("User 2 update key ...");
    user2.update_key(None).await?;
    debug!("New TK: {}", user2.art.root.public_key);
    Ok(())
}

#[tokio::test]
async fn test_delete_group() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(GROUP_SIZE).await.0;

    context.delete_group().await?;
    Ok(())
}
