use crate::user_test_model::UserTestModel;
use ark_std::rand::SeedableRng;
use ark_std::rand::prelude::StdRng;
use art::traits::{ARTPrivateAPI, ARTPrivateView, ARTPublicAPI, ARTPublicView};
use art::types::{PrivateART, PublicART};
use axum::http::StatusCode;
use bytes::{Bytes, BytesMut};
use cortado::CortadoAffine;
use crypto::schnorr::{sign, verify};
use prost::Message;
use tracing::debug;
use tracing::field::debug;
use types::art_schemas::{ChallengeResponse, GetARTResponse, ProofMode};
use types::protos;
use types::protos::{Frame, SpFrames};

const BACKEND_URL: &str = "http://localhost:8080";
const CENTRIFUGO_URL: &str = "http://localhost:8000";
// used for tests, which can be repeated
const TEST_REPEATS: usize = 1;
const DEFAULT_NONCE_LENGTH: u32 = 16; // 16 bytes
const DEFAULT_GROUP_SIZE: u64 = 100;

#[tokio::test]
async fn test_get_message() -> eyre::Result<()> {
    init_tracing_for_test();

    let (mut context, init_message) = UserTestModel::new(DEFAULT_GROUP_SIZE).await;
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

    let mut context = UserTestModel::new(DEFAULT_GROUP_SIZE).await.0;

    Ok(())
}

#[tokio::test]
async fn test_add_member() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(DEFAULT_GROUP_SIZE).await.0;
    for _ in 0..TEST_REPEATS {
        context.add_member().await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_add_member_after_removal() -> eyre::Result<()> {
    init_tracing_for_test();

    let mut context = UserTestModel::new(DEFAULT_GROUP_SIZE).await.0;

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

    let mut context = UserTestModel::new(DEFAULT_GROUP_SIZE).await.0;
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

    let mut context = UserTestModel::new(DEFAULT_GROUP_SIZE).await.0;
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

    let payload = UserTestModel::new_nonce();

    let (mut user0, _) = UserTestModel::new(DEFAULT_GROUP_SIZE).await;
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

    let payload = UserTestModel::new_nonce();

    let (mut user0, _) = UserTestModel::new(7).await;
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

    let mut context = UserTestModel::new(DEFAULT_GROUP_SIZE).await.0;

    context.delete_group().await?;
    Ok(())
}

fn init_tracing_for_test() {
    _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .try_init();
}
