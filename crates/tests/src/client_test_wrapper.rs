use std::ops::Mul;
use ark_ec::{AffineRepr, CurveGroup};
use ark_serialize::CanonicalSerialize;
use ark_std::UniformRand;
use chrono::Utc;
use ark_std::rand::prelude::{Rng, StdRng};
use axum::http::StatusCode;
use bytes::{Bytes, BytesMut};
use uuid::Uuid;
use zrt_client_sdk::{models, utils};
use cortado::{CortadoAffine, Fr};
use sha3::{Digest, Sha3_256};
use tracing::debug;
use zrt_art::art::PublicArt;
use zrt_art::errors::ArtError;
use zrt_client_sdk::contexts::group::GroupContext;
use zrt_client_sdk::contexts::invite::InviteContext;
use zrt_client_sdk::models::frame::Frame;
use zrt_client_sdk::models::invite::Invite;
use zrt_crypto::schnorr::{sign, verify};
use types::art_schemas::{ChallengeResponse, GetARTQuery, GetARTResponse, ProofMode};
use crate::{BACKEND_URL, DEFAULT_NONCE_LENGTH};

pub struct InviteClientWrapper {
    group_context: InviteContext,
}

pub struct ClientWrapper {
    group_context: GroupContext<StdRng>,
}

impl ClientWrapper {
    pub fn new_group(rng: &mut StdRng) -> (Self, Frame) {
        let group_id = Uuid::new_v4();

        let group_info =
            models::group_info::GroupInfo::new(group_id, String::from("Group"), Utc::now(), vec![]);

        let identity_secret_key = Fr::rand(rng);
        let identity_public_key = (CortadoAffine::generator() * identity_secret_key).into_affine();
        let identity_public_key_bytes =
            utils::serialize(identity_public_key).expect("Failed to serialize identity public key");

        let owner = models::group_info::User::new(String::from("Owner"), identity_public_key, vec![]);

        let (mut group_context, frame) = GroupContext::new(identity_secret_key, owner, group_info)
            .expect("Failed to create group context");

        let client_wrapper = Self {
            group_context
        };

        (client_wrapper, frame)
    }

    pub async fn send_frame(frame: Frame) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut buf = BytesMut::new();
        buf.extend(frame.encode_to_vec()?);
        // frame.encode(&mut buf).unwrap();
        let group_id = frame.frame_tbs().group_id().clone();
        let client = reqwest::Client::new();

        Ok((
            client
                .post(format!(
                    "{}/{}/{}/{}",
                    BACKEND_URL, "v1/group", group_id, "frames"
                ))
                .body(Bytes::from(buf.clone()))
                .send()
                .await?,
            buf,
        ))
    }

    pub fn process_frame(&mut self, frame: Frame) -> eyre::Result<()> {
        self.group_context.process_frame(frame)?;

        Ok(())
    }

    pub fn add_member(&mut self, member_identity_secret_key: Fr) -> eyre::Result<(Frame, Invite)> {
        let member_identity_public_key =
            (CortadoAffine::generator() * member_identity_secret_key).into_affine();

        let invitee = models::invite::Invitee::Identified {
            identity_public_key: member_identity_public_key,
            spk_public_key: None,
        };

        let (frame, invite) = self.group_context.add_member(invitee, vec![])?;

        Ok((frame, invite))
    }

    pub fn create_frame(&mut self, content: Vec<u8>) -> eyre::Result<Frame> {
        let frame = self.group_context.create_frame(content).unwrap();

        Ok(frame)
    }

    pub fn join_group(&mut self) -> eyre::Result<Frame> {
        let member =
            models::group_info::User::new(String::from("Member"), self.group_context.identity_public_key(), vec![]);
        let frame = self.group_context
            .join_group_as(member)
            .expect("Failed to propose to join group");

        Ok(frame)
    }
}

impl InviteClientWrapper {
    pub fn new(member_identity_secret_key: Fr, invite: Invite) -> Self {
        let group_context = InviteContext::new(member_identity_secret_key, None, invite)
            .expect("Failed to create invite context");

        Self { group_context }
    }

    pub async fn get_challenge(&self) -> reqwest::Result<Vec<u8>> {
        let mut serialized_public_key = Vec::new();
        self
            .group_context
            .leaf_public_key()
            .serialize_compressed(&mut serialized_public_key)
            .unwrap();

        let challenge_response = reqwest::Client::new()
            .get(format!(
                "{}/{}/{}/{}",
                BACKEND_URL, "v1/group", self.group_context.group_id(), "challenge"
            ))
            .send()
            .await?;

        assert_eq!(challenge_response.status(), StatusCode::OK);

        let challenge = challenge_response
            .json::<ChallengeResponse>()
            .await?
            .challenge;

        Ok(challenge)
    }

    pub async fn get_art(
        &self,
        epoch: u64,
        proof_mode: String,
    ) -> eyre::Result<PublicArt<CortadoAffine>> {
        // Get challenge for proof
        let challenge = self.get_challenge().await?;

        // Create signature
        let nonce = new_nonce();

        let mut msg = Vec::new();
        msg.extend_from_slice(self.group_context.group_id().as_bytes());
        msg.extend(&nonce);
        msg.extend(&challenge);
        msg.extend(epoch.to_be_bytes());

        let msg = Sha3_256::digest(&msg).to_vec();

        let signature = self.group_context.sign_as_leaf(&msg).unwrap();
        // let signature = sign(&vec![sk], &vec![pk], &msg).unwrap();
        let pk = self.group_context.leaf_public_key();
        let verification_result = verify(&signature, &vec![pk], &msg);
        assert!(verification_result.is_ok());

        let mut public_key_bytes = Vec::new();
        pk.serialize_compressed(&mut public_key_bytes).unwrap();

        let get_art_response = reqwest::Client::new()
            .get(format!(
                "{}/{}/{}/{}",
                BACKEND_URL, "v1/group", self.group_context.group_id(), epoch
            ))
            .query(&GetARTQuery {
                signature,
                nonce,
                challenge,
                proof_mode,
                public_key: public_key_bytes,
            })
            .send()
            .await?;

        assert_eq!(get_art_response.status(), StatusCode::OK);

        let received_art =
            postcard::from_bytes(&get_art_response.json::<GetARTResponse>().await?.art)
                .map_err(ArtError::from)?;

        Ok(received_art)
    }

    pub async fn apply_join_frame(self, frame: Frame) -> eyre::Result<ClientWrapper> {
        let epoch = self.group_context.epoch();
        let proof_mode = ProofMode::UseLeafKey.to_string();
        let art = self.get_art(epoch, proof_mode).await?;

        let member_group_context = self.group_context
            .upgrade(art)
            .expect("Failed to upgrade group context");

        Ok(ClientWrapper {
            group_context: member_group_context
        })
    }
}

pub fn new_nonce() -> Vec<u8> {
    std::iter::repeat_n(rand::random::<u8>(), DEFAULT_NONCE_LENGTH as usize)
        .collect::<Vec<u8>>()
}