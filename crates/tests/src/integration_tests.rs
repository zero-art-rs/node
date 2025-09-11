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
use types::art_schemas::{ChallengeResponse, GetARTResponse, ProofMode};
use types::protos;
use types::protos::{Frame, SpFrames};

const BACKEND_URL: &str = "http://localhost:8080";
const CENTRIFUGO_URL: &str = "http://localhost:8000";
// used for tests, which can be repeated
const TEST_REPEATS: usize = 4;
const DEFAULT_NONCE_LENGTH: u32 = 16; // 16 bytes
const DEFAULT_GROUP_SIZE: u64 = 10;

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
        context.make_blank(i + 1).await?;
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
        context.make_blank(i + 5).await?;

        let received_art = retrieval_context
            .get_art((i + 1) as u64, None, ProofMode::UseLeafKey.to_string())
            .await?;

        assert_eq!(
            received_art.root.weight,
            retrieval_context.art.root.weight - 1
        );

        retrieval_context.art = PrivateART::from_public_art(received_art, context.art.secret_key)?;

        let sk_to_use = retrieval_context.art.recompute_root_key()?.key;
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

        assert_eq!(received_art.root.public_key, art_roots[(i) as usize]);

        retrieval_context.art =
            PrivateART::from_public_art(received_art, retrieval_context.initial_secrets[1])?;
    }

    Ok(())
}

// #[tokio::test]
// async fn test_epoch_merge() -> eyre::Result<()> {
//     init_tracing_for_test();
//
//     let mut rng = StdRng::seed_from_u64(rand::random());
//
//     let payload = UserTestModel::new_nonce();
//
//     let (mut user0, _) = UserTestModel::new(DEFAULT_GROUP_SIZE).await;
//     let mut user1 = user0.derive_new(1)?;
//     let mut user2 = user0.derive_new(2)?;
//     let mut user3 = user0.derive_new(5)?;
//
//     // sanity check
//     assert_eq!(user2.art.get_root(), user0.art.get_root());
//     assert_eq!(user1.art.get_root(), user0.art.get_root());
//     assert_eq!(user3.art.get_root(), user0.art.get_root());
//
//     debug!("User 0 update key ...");
//     user0.update_key(Some(payload.clone())).await?;
//     let user0_pk = user0.art.public_key_of(&user0.art.get_secret_key());
//
//     debug!("User 1 update key ...");
//     user1.update_key(Some(payload.clone())).await?;
//     let user1_pk = user1.art.public_key_of(&user1.art.get_secret_key());
//
//     debug!("User 3 add member ...");
//     user3.add_member().await?;
//     let user3_pk = user3.art.public_key_of(&user3.art.get_secret_key());
//
//     // debug!("Users pull available changes ...");
//     // user0.pull_changes(1000, 0).await?;
//     // user1.pull_changes(1000, 0).await?;
//     // user3.pull_changes(1000, 0).await?;
//     // let changes = user2.pull_changes(1000, 0).await?;
//
//     debug!("User 2 merge changes locally ...");
//     let merge_options = user2.merge_changes(&changes).await?;
//
//     debug!("User 2 update and send update request with merge resolve");
//     let key_update_response2 = user2
//         .update_key_and_send_request(metadata.clone(), payload.clone(), Some(merge_options))
//         .await?;
//     assert_eq!(key_update_response2.status(), StatusCode::OK);
//
//     let merge_change = user0.pull_changes(1000, 3).await?;
//     assert_eq!(merge_change.len(), 1);
//
//     debug!("User 0 apply merge changes ...");
//     user0.apply_merge_changes(&merge_change[0]).await?;
//     assert_eq!(
//         user0.art.get_node(user0.art.get_node_index())?.public_key,
//         user0_pk
//     );
//     assert_eq!(user0.art.get_root(), user2.art.get_root());
//
//     debug!("User 1 apply merge changes ...");
//     user1.apply_merge_changes(&merge_change[0]).await?;
//     assert_eq!(
//         user1.art.get_node(user1.art.get_node_index())?.public_key,
//         user1_pk
//     );
//     assert_eq!(user1.art.get_root(), user0.art.get_root());
//
//     debug!("User 3 apply merge changes ...");
//     user3.apply_merge_changes(&merge_change[0]).await?;
//     assert_eq!(
//         user3.art.get_node(user3.art.get_node_index())?.public_key,
//         user3_pk
//     );
//     assert_eq!(user3.art.get_root(), user0.art.get_root());
//
//     debug!("User 2 update key ...");
//     user2.update_key_and_send_request(None, None, None);
//
//     let change = user0.pull_changes(1000, user0.epoch).await?;
//
//     Ok(())
// }

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
        .with_target(true)
        .try_init();
}
