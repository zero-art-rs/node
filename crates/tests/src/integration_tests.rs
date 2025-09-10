use art::traits::ARTPrivateAPI;
use art::types::{PrivateART, PublicART};
use crate::user_test_model::UserTestModel;
use axum::http::StatusCode;
use bytes::{Bytes, BytesMut};
use cortado::CortadoAffine;
use crypto::schnorr::{sign, verify};
use prost::Message;
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
    let mut retrieval_context = context.derive_new(2)?;

    // skip the root node
    for i in 0..TEST_REPEATS {
        context.make_blank(i + 1).await?;

        let received_art = retrieval_context.get_art(
            i as u64,
            None,
            ProofMode::UseLeafKey.to_string(),
        )
            .await?;

        assert_eq!(
            received_art.root.weight,
            retrieval_context.art.root.weight - 1
        );

        retrieval_context.art = PrivateART::from_public_art(received_art, context.art.secret_key)?;

        let sk_to_use = retrieval_context.art.recompute_root_key()?.key;
        let received_art_check = retrieval_context.get_art(
            i as u64,
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
        let received_art = retrieval_context.get_art(
            i as u64,
            None,
            ProofMode::UseLeafKey.to_string(),
        )
            .await?;

        assert_eq!(received_art.root.public_key, art_roots[(i) as usize]);

        retrieval_context.art =
            PrivateART::from_public_art(received_art, retrieval_context.initial_secrets[1])?;
    }

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
