use crate::utils::CentrifugoTokenResponse;
use ark_ec::{AffineRepr, CurveGroup};
use ark_ed25519::EdwardsAffine as Ed25519Affine;
use ark_serialize::CanonicalSerialize;
use ark_std::{
    UniformRand,
    rand::prelude::StdRng,
    rand::{SeedableRng, thread_rng},
};
use axum::body::Bytes;
use bulletproofs::PedersenGens;
use bytes::BytesMut;
use cortado::{CortadoAffine, Fr};
use curve25519_dalek::Scalar;
use prost::Message;
use reqwest::StatusCode;
use sha3::{Digest, Sha3_256};
use std::ops::Mul;
use tracing::{debug, error};
use types::{
    art_schemas::{ChallengeResponse, GetARTQuery, GetARTResponse},
    centrifugo_schemas::AuthRequest,
    messenger_schemas::GetMessageQuery,
    protos::{Frame, FrameTbs, GroupOperation, SpFrames, group_operation::Operation},
    utils::extract_branch_changes,
};
use utoipa::openapi::request_body::RequestBody;
use uuid::Uuid;
use zkp::toolbox::{cross_dleq::PedersenBasis, dalek_ark::ristretto255_to_ark};
use zrt_art::traits::ARTPublicView;
use zrt_art::types::{BranchChanges, Direction, NodeIndex, ProverArtefacts};
use zrt_art::{
    errors::ARTError,
    traits::{ARTPrivateAPI, ARTPrivateView, ARTPublicAPI},
    types::{PrivateART, PublicART},
};
use zrt_crypto::schnorr::{sign, verify};
use zrt_zk::art::{art_prove, art_verify};

use crate::{BACKEND_URL, DEFAULT_NONCE_LENGTH};

#[derive(Clone, Debug)]
pub struct UserTestModel {
    pub client: reqwest::Client,
    pub art: PrivateART<CortadoAffine>,
    pub initial_secrets: Vec<Fr>,
    pub chat_uuid: Uuid,
    pub epoch: u64,
    pub owner_id_key: Option<Fr>,
}

#[derive(Debug, thiserror::Error)]
pub enum UserTestModelError {
    #[error("Wrong StatusCode: {got}, while expected {expected}")]
    WrongStatusCode { got: String, expected: String },
}

fn get_co_path_values(
    art: &PrivateART<CortadoAffine>,
    index: &NodeIndex,
) -> Result<Vec<CortadoAffine>, ARTError> {
    let mut co_path_values = Vec::new();

    let mut parent = art.get_root();
    for direction in &index.get_path()? {
        co_path_values.push(parent.get_other_child(direction)?.public_key);
        parent = parent.get_child(direction)?;
    }

    co_path_values.reverse();
    Ok(co_path_values)
}

impl From<(StatusCode, StatusCode)> for UserTestModelError {
    fn from((got, expected): (StatusCode, StatusCode)) -> Self {
        Self::WrongStatusCode {
            got: got.to_string(),
            expected: expected.to_string(),
        }
    }
}
#[allow(dead_code)]
impl UserTestModel {
    pub async fn new(size: u64) -> (Self, BytesMut) {
        // let seed = rand::random();
        let seed = 0;
        let mut rng = StdRng::seed_from_u64(seed);

        let secrets = (0..size).map(|_| Fr::rand(&mut rng)).collect();
        debug!("secrets: {:?}", secrets);
        let owner_id_key = Fr::rand(&mut rng);
        let (art, _) =
            PrivateART::new_art_from_secrets(&secrets, &CortadoAffine::generator()).unwrap();

        let id = Uuid::now_v7();

        let user = Self {
            client: reqwest::Client::new(),
            art,
            initial_secrets: secrets,
            chat_uuid: id,
            epoch: 0, // init request is already one epoch
            owner_id_key: Some(owner_id_key),
        };

        // Create new_group for testing
        let (response, init_message) = user
            .create_new_chat(
                PublicART::new_art_from_secrets(&user.initial_secrets, &CortadoAffine::generator())
                    .unwrap()
                    .0,
                owner_id_key,
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        (user, init_message)
    }

    pub fn index_of(&self, member_id: usize) -> Result<NodeIndex, ARTError> {
        Ok(NodeIndex::from(self.art.get_path_to_leaf(
            &self.art.public_key_of(&self.initial_secrets[member_id]),
        )?))
    }

    /// Clone this uses, and change this user secret key to the different one
    pub fn derive_new(&self, index: usize) -> Result<Self, ARTError> {
        let art = PrivateART::from_public_art(self.art.clone(), self.initial_secrets[index])?;

        Ok(Self {
            client: reqwest::Client::new(),
            art,
            initial_secrets: self.initial_secrets.clone(),
            chat_uuid: self.chat_uuid,
            epoch: self.epoch,
            owner_id_key: None,
        })
    }

    pub const fn is_owner(&self) -> bool {
        self.owner_id_key.is_some()
    }

    pub async fn create_new_chat(
        &self,
        art: PublicART<CortadoAffine>,
        sk: Fr,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let pk = art.public_key_of(&sk);
        let mut serialized_pk = Vec::new();
        pk.serialize_compressed(&mut serialized_pk)?;

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: 0,
            nonce: serialized_pk,
            group_operation: Some(GroupOperation {
                operation: Some(Operation::Init(art.serialize()?)),
            }),
            protected_payload: vec![],
        };

        let mut msg = BytesMut::new();
        tbs_frame.encode(&mut msg)?;

        let msg = Sha3_256::digest(&msg);

        let signature = sign(&vec![sk], &vec![pk], &msg)?;
        let verification_result = verify(&signature, &vec![pk], &msg);
        assert!(verification_result.is_ok());

        let req = Frame {
            frame: Some(tbs_frame),
            proof: signature,
        };

        self.send_frame(req).await
    }

    fn get_pedersen_basis() -> PedersenBasis<CortadoAffine, Ed25519Affine> {
        let gens = PedersenGens::default();
        PedersenBasis::<CortadoAffine, Ed25519Affine>::new(
            CortadoAffine::generator(),
            CortadoAffine::new_unchecked(cortado::ALT_GENERATOR_X, cortado::ALT_GENERATOR_Y),
            ristretto255_to_ark(gens.B).unwrap(),
            ristretto255_to_ark(gens.B_blinding).unwrap(),
        )
    }

    pub async fn update_key(
        &mut self,
        payload: Option<Vec<u8>>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let secret_key = self.art.secret_key;
        let new_secret_key = Fr::rand(&mut rng);

        let mut art_clone = self.art.clone();
        let (_, key_update_changes, artefacts) = art_clone.update_key(&new_secret_key)?;

        debug!(
            "UpdateKey debug data:\n\tepoch: {}\n\tNew TK: {}",
            self.epoch + 1,
            self.art.root.public_key
        );

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::KeyUpdate(key_update_changes.serialze()?)),
            }),
            protected_payload: payload.unwrap_or_default(),
        };

        let proof_bytes = self.prove_and_check_art_update(
            &art_clone,
            secret_key,
            &artefacts,
            &tbs_frame,
            &key_update_changes,
        )?;

        let update_key_response = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        if update_key_response.0.status() != StatusCode::OK {
            Err(UserTestModelError::from((
                update_key_response.0.status(),
                StatusCode::OK,
            )))?;
        }

        self.art = art_clone;
        self.epoch += 1;

        Ok(update_key_response)
    }

    // add node to the art, and send updates to the chat
    pub async fn add_member(
        &mut self,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        // let old_tk = self.art.get_root_key()?.key;
        let old_tk = self.art.secret_key;
        let new_user_secret_key = Fr::rand(&mut rng);
        let mut art_clone = self.art.clone();
        let (_, append_user_changes, artefacts) =
            art_clone.append_or_replace_node(&new_user_secret_key)?;

        debug!(
            "UpdateKey debug data:\n\tepoch: {}\n\tNew TK: {}\n\tstatus_check: {:?}",
            self.epoch + 1,
            self.art.root.public_key,
            status_check,
        );

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::AddMember(append_user_changes.serialze()?)),
            }),
            protected_payload: vec![],
        };

        let proof_bytes = self.prove_and_check_art_update(
            &art_clone,
            old_tk,
            &artefacts,
            &tbs_frame,
            &append_user_changes,
        )?;

        let (request_response, request_bytes) = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        if let Some(status_code) = status_check {
            assert_eq!(request_response.status(), status_code);
        }

        self.art = art_clone;
        self.epoch += 1;

        Ok((request_response, request_bytes))
    }

    pub async fn make_blank(
        &mut self,
        user_to_remove: &Vec<Direction>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let old_tk = if self.is_owner() {
            self.art.secret_key
        } else {
            self.art.get_root_key()?.key
        };

        let temporary_secret_key = Fr::rand(&mut rng);
        debug!("temporary_secret_key: {}", temporary_secret_key);
        debug!("target_node_path: {:#?}", user_to_remove);
        let mut art_clone = self.art.clone();
        let (_, remove_user_changes, artefacts) =
            art_clone.make_blank(user_to_remove, &temporary_secret_key)?;

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::RemoveMember(remove_user_changes.serialze()?)),
            }),
            protected_payload: vec![],
        };

        let proof_bytes = self.prove_and_check_art_update(
            &art_clone,
            old_tk,
            &artefacts,
            &tbs_frame,
            &remove_user_changes,
        )?;

        let make_blank_result = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        assert_eq!(
            make_blank_result.0.status(),
            StatusCode::NO_CONTENT,
            "Check if remove member is successful."
        );

        self.art = art_clone;
        self.epoch += 1;

        Ok(make_blank_result)
    }

    pub async fn get_messages(&self, limit: i64, skip: i64) -> eyre::Result<SpFrames> {
        let sk = self.art.get_root_key()?.key;
        let pk = self.art.public_key_of(&sk);

        let nonce = Self::new_nonce();

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_uuid.as_bytes());
        msg.extend(&nonce);

        let msg = Sha3_256::digest(&msg).to_vec();

        let signature = sign(&vec![sk], &vec![pk], &msg)?;
        let verification_result = verify(&signature, &vec![pk], &msg);
        assert!(verification_result.is_ok());

        let get_messages_response = self
            .client
            .get(format!(
                "{}/{}/{}/{}",
                BACKEND_URL, "v1/group", self.chat_uuid, "frames"
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
            .await?;

        assert_eq!(get_messages_response.status(), StatusCode::ACCEPTED);

        Ok(SpFrames::decode(BytesMut::from(
            &*get_messages_response.bytes().await?,
        ))?)
    }

    // pub fn update_with(sp_frames: SpFrames) -> eyre::Result<()> {
    //
    // }

    pub async fn get_challenge(&self) -> reqwest::Result<Vec<u8>> {
        let mut serialized_public_key = Vec::new();
        self.art
            .public_key_of(&self.art.secret_key)
            .serialize_compressed(&mut serialized_public_key)
            .unwrap();

        let challenge_response = self
            .client
            .get(format!(
                "{}/{}/{}/{}",
                BACKEND_URL, "v1/group", self.chat_uuid, "challenge"
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
        secret_key_to_use: Option<Fr>,
        proof_mode: String,
    ) -> eyre::Result<PublicART<CortadoAffine>> {
        // Get challenge for proof
        let challenge = self.get_challenge().await?;

        // Create signature
        let nonce = Self::new_nonce();

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_uuid.as_bytes());
        msg.extend(&nonce);
        msg.extend(&challenge);
        msg.extend(epoch.to_be_bytes());

        let msg = Sha3_256::digest(&msg).to_vec();

        let sk = secret_key_to_use.unwrap_or(self.art.secret_key);
        // let sk = match secret_key_to_use {
        //     Some(secret_key) => secret_key,
        //     None => self.art.secret_key,
        // };

        let pk = self.art.public_key_of(&sk);

        let signature = sign(&vec![sk], &vec![pk], &msg).unwrap();
        let verification_result = verify(&signature, &vec![pk], &msg);
        assert!(verification_result.is_ok());

        let mut public_key_bytes = Vec::new();
        pk.serialize_compressed(&mut public_key_bytes).unwrap();

        let get_art_response = self
            .client
            .get(format!(
                "{}/{}/{}/{}",
                BACKEND_URL, "v1/group", self.chat_uuid, epoch
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

        let received_art = PublicART::<CortadoAffine>::deserialize(
            &get_art_response.json::<GetARTResponse>().await?.art,
        )?;

        Ok(received_art)
    }

    pub async fn delete_group(&mut self) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let nonce = Self::new_nonce();

        let challenge = self.get_challenge().await?;

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce,
            group_operation: Some(GroupOperation {
                operation: Some(Operation::DropGroup(challenge)),
            }),
            protected_payload: vec![],
        };

        let mut buf = BytesMut::new();
        tbs_frame.encode(&mut buf).unwrap();
        let msg = &*Sha3_256::digest(&*buf).to_vec();

        let pk = vec![self.art.public_key_of(&self.art.secret_key)];
        let signature = sign(&vec![self.art.secret_key], &pk, msg).unwrap();
        let verification_result = verify(&signature, &pk, msg);
        assert!(verification_result.is_ok());

        let delete_response = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: signature,
            })
            .await?;

        assert_eq!(delete_response.0.status(), StatusCode::NO_CONTENT);
        self.epoch += 1;

        Ok(delete_response)
    }

    fn sign_and_check_signature(&self, msg: &[u8], sk: Fr) -> eyre::Result<Vec<u8>> {
        let pk = self.art.public_key_of(&sk);

        let signature = sign(&vec![sk], &vec![pk], msg)?;
        let verification_result = verify(&signature, &vec![pk], msg);
        assert!(verification_result.is_ok(), "Failed to verify signature");

        Ok(signature)
    }

    fn prove_and_check_art_update(
        &self,
        test_art: &PrivateART<CortadoAffine>,
        secret_key: Fr,
        artefacts: &ProverArtefacts<CortadoAffine>,
        tbs_frame: &FrameTbs,
        changes: &BranchChanges<CortadoAffine>,
    ) -> eyre::Result<Vec<u8>> {
        let mut buf = BytesMut::new();
        tbs_frame.encode(&mut buf)?;
        let associated_data = &*Sha3_256::digest(&buf).to_vec();

        let blindings: Vec<_> = (0..=artefacts.co_path.len())
            .map(|_| Scalar::random(&mut thread_rng()))
            .collect();

        let public_key = CortadoAffine::generator().mul(secret_key).into_affine();

        debug!("Using public_key: {} for proof creation.", &public_key);

        let proof = art_prove(
            Self::get_pedersen_basis(),
            associated_data,
            vec![public_key],
            artefacts.path.clone(),
            artefacts.co_path.clone(),
            artefacts.secrets.clone(),
            vec![secret_key],
            blindings,
        )?;

        let verification_result = art_verify(
            Self::get_pedersen_basis(),
            associated_data,
            vec![public_key],
            changes.public_keys.iter().rev().copied().collect(),
            get_co_path_values(&test_art, &changes.node_index)?,
            proof.clone(),
        )
        .is_ok();

        assert!(verification_result);

        let mut proof_bytes = Vec::new();
        proof.serialize_compressed(&mut proof_bytes)?;

        Ok(proof_bytes)
    }

    pub async fn get_changes(
        &self,
        limit: i64,
        skip: i64,
        epoch: Option<u64>,
    ) -> eyre::Result<Vec<BranchChanges<CortadoAffine>>> {
        let tk = self.art.get_root_key()?.key;
        let pk = self.art.root.public_key;

        let mut msg = Vec::new();
        let nonce = (0..DEFAULT_NONCE_LENGTH)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<u8>>();
        msg.extend_from_slice(self.chat_uuid.as_bytes());
        msg.extend(&nonce);

        let msg = Sha3_256::digest(&msg).to_vec();

        debug!("Using {} for verification.", pk);

        let signature = sign(&vec![tk], &vec![pk], &msg).unwrap();

        assert!(verify(&signature, &vec![pk], &msg).is_ok());

        let changes_response = self
            .client
            .get(format!(
                "{}/{}/{}/{}",
                BACKEND_URL, "v1/group", self.chat_uuid, "frames"
            ))
            .query(&GetMessageQuery {
                message_sequence_number: None,
                signature,
                limit,
                skip,
                nonce,
                epoch,
            })
            .send()
            .await?;

        assert_eq!(changes_response.status(), StatusCode::ACCEPTED);

        let buf = BytesMut::from(&*changes_response.bytes().await?);
        let sp_frames = SpFrames::decode(buf)?.sp_frames;

        let mut changes = Vec::with_capacity(sp_frames.len());
        for sp_frame in sp_frames {
            let Some(frame) = sp_frame.frame else {
                continue;
            };

            if let Some(frame_change) = extract_branch_changes(&frame)? {
                debug!("frame_change: {:#?}", frame_change);
                changes.push(frame_change);
            }
        }

        Ok(changes)
    }

    pub fn new_nonce() -> Vec<u8> {
        std::iter::repeat_n(rand::random::<u8>(), DEFAULT_NONCE_LENGTH as usize)
            .collect::<Vec<u8>>()
    }

    /// returns response from the server and the message, which was sent
    pub async fn send_frame(&self, frame: Frame) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut buf = BytesMut::new();
        frame.encode(&mut buf).unwrap();

        Ok((
            self.client
                .post(format!(
                    "{}/{}/{}/{}",
                    BACKEND_URL, "v1/group", self.chat_uuid, "frames"
                ))
                .body(Bytes::from(buf.clone()))
                .send()
                .await?,
            buf,
        ))
    }

    pub async fn get_centrifugo_token(
        &self,
        request: AuthRequest,
    ) -> eyre::Result<CentrifugoTokenResponse> {
        let centrifugo_token_response = self
            .client
            .post(format!("{}/{}", BACKEND_URL, "centrifugo/auth"))
            .json(&request)
            .send()
            .await?;

        assert_eq!(centrifugo_token_response.status(), StatusCode::OK);

        let centrifugo_token_response = centrifugo_token_response
            .json::<CentrifugoTokenResponse>()
            .await?;

        Ok(centrifugo_token_response)
    }
}
