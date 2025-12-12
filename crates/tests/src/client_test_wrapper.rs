use crate::{BACKEND_URL, DEFAULT_NONCE_LENGTH};
use ark_ec::{AffineRepr, CurveGroup};
use ark_serialize::CanonicalSerialize;
use ark_std::UniformRand;
use ark_std::rand::prelude::{Rng, StdRng};
use axum::http::StatusCode;
use bytes::{Bytes, BytesMut};
use chrono::Utc;
use cortado::{CortadoAffine, Fr};
use prost::Message;
use sha3::{Digest, Sha3_256};
use std::ops::Mul;
use tracing::{debug, error, warn};
use types::DEFAULT_LIMIT;
use types::art_schemas::{ChallengeResponse, GetARTQuery, GetARTResponse, ProofMode};
use types::messenger_schemas::GetMessageQuery;
use uuid::Uuid;
use zrt_art::art::PublicArt;
use zrt_art::errors::ArtError;
use zrt_client_sdk::contexts::group::GroupContext;
use zrt_client_sdk::contexts::invite::InviteContext;
use zrt_client_sdk::models::frame::{Frame, FrameTbs, Proof};
use zrt_client_sdk::models::invite::Invite;
use zrt_client_sdk::zero_art_proto::SpFrames;
use zrt_client_sdk::{models, utils};
use zrt_crypto::schnorr::{sign, verify};
use crate::utils::ArrayLessPrinter;

pub struct InviteClientWrapper {
    group_context: InviteContext,
}

pub struct ClientWrapper {
    group_context: GroupContext<StdRng>,
    sequence_number: Option<i64>,
}

impl ClientWrapper {
    pub fn new(group_context: GroupContext<StdRng>) -> Self {
        Self {
            group_context,
            sequence_number: None,
        }
    }
    pub fn new_group(rng: &mut StdRng) -> (Self, Frame) {
        let group_id = Uuid::new_v4();

        let group_info =
            models::group_info::GroupInfo::new(group_id, String::from("Group"), Utc::now(), vec![]);

        let identity_secret_key = Fr::rand(rng);
        let identity_public_key = (CortadoAffine::generator() * identity_secret_key).into_affine();
        let identity_public_key_bytes =
            utils::serialize(identity_public_key).expect("Failed to serialize identity public key");

        let owner =
            models::group_info::User::new(String::from("Owner"), identity_public_key, vec![]);

        let (group_context, frame) = GroupContext::new(identity_secret_key, owner, group_info)
            .expect("Failed to create group context");

        let client_wrapper = Self {
            group_context,
            sequence_number: None,
        };

        (client_wrapper, frame)
    }

    pub fn group_context(&self) -> &GroupContext<StdRng> {
        &self.group_context
    }

    pub async fn send_frame(frame: Frame) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut buf = BytesMut::new();
        buf.extend(frame.encode_to_vec()?);
        // frame.encode(&mut buf).unwrap();
        let group_id = frame.frame_tbs().group_id().clone();

        debug!(
            frame = ?ArrayLessPrinter::from(&frame),
            group_id = ?group_id,
            "Sending frame",
        );
        Ok((
            reqwest::Client::new()
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
        debug!(
            epoch = ?self.group_context.epoch(),
            "create_frame"
        );

        let frame = self.group_context.create_frame(content).unwrap();

        Ok(frame)
    }

    pub fn join_group(&mut self) -> eyre::Result<Frame> {
        let member = models::group_info::User::new(
            String::from("Member"),
            self.group_context.identity_public_key(),
            vec![],
        );
        let frame = self
            .group_context
            .join_group_as(member)
            .expect("Failed to propose to join group");

        Ok(frame)
    }

    pub async fn get_messages(&self, limit: i64, skip: i64) -> eyre::Result<SpFrames> {
        let nonce = new_nonce();

        let mut msg = Vec::new();
        msg.extend_from_slice(self.group_context.group_info().id().as_bytes());
        msg.extend(&nonce);
        let msg = Sha3_256::digest(&msg).to_vec();

        let signature = self.group_context.sign_with_tk(&msg, true).unwrap();
        let query = GetMessageQuery {
            message_sequence_number: self.sequence_number.map(|sn| sn + 1),
            limit,
            skip: 0,
            signature,
            nonce: nonce.clone(),
            epoch: Some(self.group_context.epoch()),
            use_upstream_key: true,
        };
        let mut get_messages_response = reqwest::Client::new()
            .get(format!(
                "{}/{}/{}/{}",
                BACKEND_URL,
                "v1/group",
                self.group_context.group_info().id(),
                "frames"
            ))
            .query(&query)
            .send()
            .await?;

        // retry
        if !matches!(get_messages_response.status(), StatusCode::ACCEPTED) {
            let signature = self.group_context.sign_with_tk(&msg, false).unwrap();
            let query = GetMessageQuery {
                message_sequence_number: self.sequence_number.map(|sn| sn + 1),
                limit,
                skip: 0,
                signature,
                nonce,
                epoch: Some(self.group_context.epoch()),
                use_upstream_key: false,
            };
            get_messages_response = reqwest::Client::new()
                .get(format!(
                    "{}/{}/{}/{}",
                    BACKEND_URL,
                    "v1/group",
                    self.group_context.group_info().id(),
                    "frames"
                ))
                .query(&query)
                .send()
                .await?;

            if !matches!(get_messages_response.status(), StatusCode::ACCEPTED) {
                return Err(eyre::eyre!(
                    "Invalid response status: status_received: {:?}, expected: {:?}",
                    get_messages_response.status(),
                    StatusCode::ACCEPTED
                ));
            }
        }

        Ok(SpFrames::decode(BytesMut::from(
            &*get_messages_response.bytes().await?,
        ))?)
    }

    pub fn update_sequence_number(&mut self, sequence_number: i64) {
        match self.sequence_number {
            None => self.sequence_number = Some(sequence_number),
            Some(inner_sequence_number) => {
                if inner_sequence_number < sequence_number {
                    self.sequence_number = Some(sequence_number);
                }
            }
        }
    }

    pub async fn poll(&mut self) -> eyre::Result<()> {
        let mut skip = 0;
        let mut sp_frames = self
            .get_messages(DEFAULT_LIMIT, skip)
            .await
            .inspect_err(|err| error!(
                DEFAULT_LIMIT = ?DEFAULT_LIMIT,
                skip = ?skip,
                "Fail to get messages: {err}"
            ))?
            .sp_frames;

        while !sp_frames.is_empty() {
            let sp_frames_len = sp_frames.len();
            for sp_frame in sp_frames {
                if let Some(frame) = sp_frame.frame {
                    debug!(frame = ?ArrayLessPrinter::from(&frame), "Processing frame");
                    let (payload, sender, verified) = self
                        .group_context
                        .process_frame(Frame::try_from(frame).expect("Failed to parse frame"))
                        .inspect_err(|err| {
                            error!(
                                "fail to process frame: {:?}. Art preview:\n{}",
                                err,
                                self.group_context.tree().preview().root()
                            )
                        })?;
                    debug!(
                        group_info = ?self.group_context.group_info(),
                        "Frame processed"
                    );

                    self.update_sequence_number(sp_frame.seq_num as i64);
                }
            }

            if sp_frames_len < DEFAULT_LIMIT as usize {
                break;
            }

            skip += DEFAULT_LIMIT;
            sp_frames = self
                .get_messages(DEFAULT_LIMIT, skip)
                .await
                .inspect_err(|err| error!(
                    err = ?err,
                    DEFAULT_LIMIT = ?DEFAULT_LIMIT,
                    skip = ?skip,
                    "Fail to get messages"
                ))?
                .sp_frames;
        }

        Ok(())
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
        self.group_context
            .leaf_public_key()
            .serialize_compressed(&mut serialized_public_key)
            .unwrap();

        let challenge_response = reqwest::Client::new()
            .get(format!(
                "{}/{}/{}/{}",
                BACKEND_URL,
                "v1/group",
                self.group_context.group_id(),
                "challenge"
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
                BACKEND_URL,
                "v1/group",
                self.group_context.group_id(),
                epoch
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

        assert_eq!(
            get_art_response.status(),
            StatusCode::OK,
            "Failed to get art"
        );
        warn!("get_art_response.status(): {:?}", get_art_response.status());

        let received_art =
            postcard::from_bytes(&get_art_response.json::<GetARTResponse>().await?.art)
                .map_err(ArtError::from)?;

        Ok(received_art)
    }

    pub async fn apply_join_frame(self, frame: Frame) -> eyre::Result<ClientWrapper> {
        let epoch = self.group_context.epoch();
        let proof_mode = ProofMode::UseLeafKey.to_string();
        let mut art = self.get_art(epoch, proof_mode).await?;
        art.commit()?;

        let member_group_context = self
            .group_context
            .upgrade(art)
            .expect("Failed to upgrade group context");

        Ok(ClientWrapper {
            group_context: member_group_context,
            sequence_number: None,
        })
    }
}

pub fn new_nonce() -> Vec<u8> {
    std::iter::repeat_n(rand::random::<u8>(), DEFAULT_NONCE_LENGTH as usize).collect::<Vec<u8>>()
}
