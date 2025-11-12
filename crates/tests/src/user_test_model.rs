use crate::utils::{CentrifugoTokenResponse, stringify_option};
use crate::{BACKEND_URL, DEFAULT_NONCE_LENGTH};
use ark_ec::{AffineRepr, CurveGroup};
use ark_ed25519::EdwardsAffine as Ed25519Affine;
use ark_serialize::CanonicalSerialize;
use ark_std::rand::Rng;
use ark_std::rand::prelude::ThreadRng;
use ark_std::{UniformRand, rand::SeedableRng, rand::prelude::StdRng};
use axum::body::Bytes;
use bulletproofs::PedersenGens;
use bytes::BytesMut;
use cortado::{CortadoAffine, Fr};
use prost::Message;
use reqwest::StatusCode;
use sha3::{Digest, Sha3_256};
use std::ops::Mul;
use tracing::{debug, error};
use types::protos::SpFrame;
use types::{
    art_schemas::{ChallengeResponse, GetARTQuery, GetARTResponse},
    centrifugo_schemas::AuthRequest,
    messenger_schemas::GetMessageQuery,
    protos::{Frame, FrameTbs, GroupOperation, SpFrames, group_operation::Operation},
    utils::extract_branch_changes,
};
use uuid::Uuid;
use zkp::rand::thread_rng;
use zkp::toolbox::{cross_dleq::PedersenBasis, dalek_ark::ristretto255_to_ark};
use zrt_art::art_node::{TreeMethods};
use zrt_art::art::{AggregationContext, ArtAdvancedOps, PrivateZeroArt, PrivateArt, PublicArt};
use zrt_art::changes::aggregations::AggregatedChange;
use zrt_art::changes::branch_change::BranchChange;
use zrt_art::changes::{ApplicableChange, ProvableChange};
use zrt_art::node_index::{Direction, NodeIndex};
use zrt_art::errors::ArtError;
use zrt_crypto::schnorr::{sign, verify};
use zrt_zk::{EligibilityRequirement, art::ArtProof};

pub struct UserTestModel {
    pub client: reqwest::Client,
    pub art: PrivateZeroArt<CortadoAffine, ThreadRng>,
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
        let seed = 0;
        let mut rng = StdRng::seed_from_u64(seed);

        let secrets = (0..size).map(|_| Fr::rand(&mut rng)).collect::<Vec<_>>();
        debug!("Group secrets: {:#?}", secrets);
        let owner_id_key = Fr::rand(&mut rng);
        let art = PrivateArt::setup(&secrets).unwrap();
        let public_art = art.get_public_art().clone();

        let id = Uuid::now_v7();

        let user = Self {
            client: reqwest::Client::new(),
            art: PrivateZeroArt::new(art, Box::new(thread_rng())).unwrap(),
            initial_secrets: secrets,
            chat_uuid: id,
            epoch: 0, // init request is already one epoch
            owner_id_key: Some(owner_id_key),
        };

        debug!(
            "Root secret: {}...",
            stringify_option(public_art.get_root().get_public_key().x().as_ref())
        );

        // Create new_group for testing
        let (response, init_message) = user
            .create_new_chat(public_art, owner_id_key)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        (user, init_message)
    }

    pub fn index_of(&self, member_id: usize) -> Result<NodeIndex, ArtError> {
        Ok(NodeIndex::from(
            self.art.get_path_to_leaf_with(
                CortadoAffine::generator()
                    .mul(self.initial_secrets[member_id])
                    .into_affine(),
            )?,
        ))
    }

    /// Clone this uses, and change this user secret key to the different one
    pub fn derive_new(&self, index: usize) -> Result<Self, ArtError> {
        let art = PrivateArt::new(
            self.art.get_base_art().get_public_art().clone(),
            self.initial_secrets[index],
        )?;

        Ok(Self {
            client: reqwest::Client::new(),
            art: PrivateZeroArt::new(art, Box::new(thread_rng())).unwrap(),
            initial_secrets: self.initial_secrets.clone(),
            chat_uuid: self.chat_uuid,
            epoch: self.epoch,
            owner_id_key: None,
        })
    }

    pub fn clone_without_rng(&self, rng: Box<ThreadRng>) -> Self {
        Self {
            client: self.client.clone(),
            art: self.art.clone_without_rng(rng),
            initial_secrets: self.initial_secrets.clone(),
            chat_uuid: self.chat_uuid.clone(),
            epoch: self.epoch.clone(),
            owner_id_key: self.owner_id_key.clone(),
        }
    }

    pub const fn is_owner(&self) -> bool {
        self.owner_id_key.is_some()
    }

    pub async fn create_new_chat(
        &self,
        art: PublicArt<CortadoAffine>,
        sk: Fr,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let pk = CortadoAffine::generator().mul(sk).into_affine();
        let mut serialized_pk = Vec::new();
        pk.serialize_compressed(&mut serialized_pk)?;

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: 0,
            nonce: serialized_pk,
            group_operation: Some(GroupOperation {
                operation: Some(Operation::Init(postcard::to_allocvec(&art)?)),
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
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let new_secret_key = Fr::rand(&mut rng);

        let mut zero_art = self.art.clone_without_rng(Box::new(thread_rng()));
        zero_art.commit().unwrap();

        debug!(
            "UpdateKey creation debug data:\n\tnew epoch: {}\n\tOld Tk: {:#?}...\n\tOld commited Tk: {:#?}...",
            self.epoch + 1,
            stringify_option(self.art.get_root_public_key().x().as_ref()),
            stringify_option(zero_art.get_root_public_key().x().as_ref()),
        );

        let branch_change_output = zero_art.update_key(new_secret_key)?;
        let branch_change = branch_change_output.get_branch_change().clone();

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::KeyUpdate(postcard::to_allocvec(&branch_change)?)),
            }),
            protected_payload: payload.unwrap_or_default(),
        };

        let mut buf = BytesMut::new();
        tbs_frame.encode(&mut buf)?;
        let associated_data = &Sha3_256::digest(&buf).to_vec();

        let mut proof_bytes = Vec::new();
        let proof = branch_change_output.prove(associated_data, None)?;
        proof.serialize_compressed(&mut proof_bytes)?;
        let update_key_response = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        if let Some(status_check) = status_check {
            if update_key_response.0.status() != status_check {
                Err(UserTestModelError::from((
                    update_key_response.0.status(),
                    status_check,
                )))?;
            }
        }

        branch_change_output.apply(&mut zero_art).unwrap();

        debug!(
            "UpdateKey apply debug data:\n\tnew epoch: {}\n\tNew TK: {:#?}...",
            self.epoch + 1,
            stringify_option(
                zero_art
                    .get_upstream_art()
                    .get_root_public_key()
                    .x()
                    .as_ref()
            ),
        );

        self.art = zero_art;
        self.epoch += 1;

        Ok(update_key_response)
    }

    pub async fn send_aggregation<R>(
        &mut self,
        agg: &AggregationContext<PrivateArt<CortadoAffine>, CortadoAffine, R>,
        mut zero_art: PrivateZeroArt<CortadoAffine, ThreadRng>,
        payload: Option<Vec<u8>>,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)>
    where
        R: Rng + ?Sized,
    {
        debug!(
            "SendAggregation debug data:\n\
            \tepoch: {}\n\
            \tNew TK: {:#?}",
            self.epoch + 1,
            self.art.get_root().get_public_key()
        );

        let aggregation_change: AggregatedChange<CortadoAffine> = AggregatedChange::try_from(agg)?;

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::Aggregated(postcard::to_allocvec(
                    &aggregation_change,
                )?)),
            }),
            protected_payload: payload.unwrap_or_default(),
        };

        let mut buf = BytesMut::new();
        tbs_frame.encode(&mut buf)?;
        let associated_data = &Sha3_256::digest(&buf).to_vec();

        let mut proof_bytes = Vec::new();
        let proof = agg.prove(associated_data, None)?;
        proof.serialize_compressed(&mut proof_bytes)?;

        let update_key_response = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        if let Some(status_check) = status_check {
            if update_key_response.0.status() != status_check {
                Err(UserTestModelError::from((
                    update_key_response.0.status(),
                    status_check,
                )))?;
            }
        }

        aggregation_change.apply(&mut zero_art).unwrap();

        debug!(
            "UpdateKey apply debug data:\n\tnew epoch: {}\n\tNew TK: {:#?}...",
            self.epoch + 1,
            stringify_option(
                zero_art
                    .get_upstream_art()
                    .get_root_public_key()
                    .x()
                    .as_ref()
            ),
        );

        self.art = zero_art;
        self.epoch += 1;

        Ok(update_key_response)
    }

    // add node to the art, and send updates to the chat
    pub async fn send_payload(
        &mut self,
        payload: Vec<u8>,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        debug!(
            "Send payload debug data:\n\tepoch: {}\n\tNew TK: {:#?}\n\tstatus_check: {:?}",
            self.epoch,
            self.art.get_root().get_public_key(),
            status_check,
        );

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch,
            nonce: (0..DEFAULT_NONCE_LENGTH)
                .map(|_| rand::random::<u8>())
                .collect::<Vec<u8>>(),
            group_operation: None,
            protected_payload: payload,
        };

        let msg = Sha3_256::digest(tbs_frame.encode_to_vec()).to_vec();
        let tk = self.art.get_root_secret_key();
        let pk = vec![self.art.get_root().get_public_key()];

        let signature = sign(&vec![tk], &pk, &msg)?;

        let (request_response, request_bytes) = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: signature,
            })
            .await?;

        if let Some(status_check) = status_check {
            if request_response.status() != status_check {
                Err(UserTestModelError::from((
                    request_response.status(),
                    status_check,
                )))?;
            }
        }

        Ok((request_response, request_bytes))
    }

    // add node to the art, and send updates to the chat
    pub async fn add_member(
        &mut self,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let new_user_secret_key = Fr::rand(&mut rng);

        let mut zero_art = self.art.clone_without_rng(Box::new(thread_rng()));
        zero_art.commit().unwrap();

        debug!(
            "AddMember creation debug data:\n\tepoch: {}\n\tNew TK: {:#?}\n\tstatus_check: {:?}",
            self.epoch + 1,
            stringify_option(self.art.get_base_art().get_root_public_key().x().as_ref()),
            status_check,
        );

        let append_user_changes_output = zero_art.add_member(new_user_secret_key)?;
        let append_user_changes = append_user_changes_output.get_branch_change().clone();

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::AddMember(postcard::to_allocvec(
                    &append_user_changes,
                )?)),
            }),
            protected_payload: vec![],
        };

        let mut buf = BytesMut::new();
        tbs_frame.encode(&mut buf)?;
        let associated_data = &Sha3_256::digest(&buf).to_vec();

        let mut proof_bytes = Vec::new();
        let proof = append_user_changes_output.prove(associated_data, None)?;
        proof.serialize_compressed(&mut proof_bytes)?;

        let (request_response, request_bytes) = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        if let Some(status_code) = status_check {
            assert_eq!(request_response.status(), status_code);
        }

        append_user_changes.apply(&mut zero_art)?;

        debug!(
            "AddMember apply debug data:\n\tnew epoch: {}\n\tNew TK: {:#?}...",
            self.epoch + 1,
            stringify_option(
                zero_art
                    .get_upstream_art()
                    .get_root_public_key()
                    .x()
                    .as_ref()
            ),
        );

        self.art = zero_art;
        self.epoch += 1;

        Ok((request_response, request_bytes))
    }

    pub async fn make_blank(
        &mut self,
        user_to_remove: &Vec<Direction>,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let user_to_remove_index = NodeIndex::from(user_to_remove.clone());
        let mut rng = StdRng::seed_from_u64(rand::random());
        let temporary_secret_key = Fr::rand(&mut rng);

        debug!(
            "MakeBlank creation debug data:
            epoch: {}
            Old TK: {}
            temporary_secret_key: {}
            target_node_path: {:?}
            ",
            self.epoch + 1,
            stringify_option(self.art.get_base_art().get_root_public_key().x().as_ref()),
            stringify_option(Some(&temporary_secret_key)),
            user_to_remove
        );

        let mut zero_art = self.art.clone_without_rng(Box::new(thread_rng()));
        zero_art.commit().unwrap();

        // let mut art_clone = self.art.clone();
        let remove_user_changes_output =
            zero_art.remove_member(&user_to_remove_index, temporary_secret_key)?;
        let remove_user_changes = remove_user_changes_output.get_branch_change().clone();

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::RemoveMember(postcard::to_allocvec(
                    &remove_user_changes,
                )?)),
            }),
            protected_payload: vec![],
        };

        let mut buf = BytesMut::new();
        tbs_frame.encode(&mut buf)?;
        let associated_data = &Sha3_256::digest(&buf).to_vec();

        let mut proof_bytes = Vec::new();
        let proof = remove_user_changes_output.prove(associated_data, None)?;
        proof.serialize_compressed(&mut proof_bytes)?;

        let make_blank_result = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        // if let Some(status_code) = status_check {
        //     assert_eq!(
        //         make_blank_result.0.status(),
        //         status_code,
        //         "Check if remove member result status is correct."
        //     );
        // }

        if let Some(status_check) = status_check {
            if make_blank_result.0.status() != status_check {
                Err(UserTestModelError::from((
                    make_blank_result.0.status(),
                    status_check,
                )))?;
            }
        }

        remove_user_changes.apply(&mut zero_art)?;
        debug!(
            "RemoveMember apply debug data:\n\tnew epoch: {}\n\tNew TK from change: {:#?}...",
            self.epoch + 1,
            stringify_option(
                zero_art
                    .get_upstream_art()
                    .get_root_public_key()
                    .x()
                    .as_ref()
            ),
        );

        self.art = zero_art;
        self.epoch += 1;

        Ok(make_blank_result)
    }

    pub async fn leave_group(
        &mut self,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let new_secret_key = Fr::rand(&mut rng);

        // let mut art_clone = self.art.clone();
        let mut zero_art = self.art.clone_without_rng(Box::new(thread_rng()));
        zero_art.commit().unwrap();

        debug!(
            "LeaveGroup creation debug data:\n\tnew epoch: {}\n\tOld Tk: {:#?}...\n\tOld commited Tk: {:#?}...",
            self.epoch + 1,
            stringify_option(self.art.get_root_public_key().x().as_ref()),
            stringify_option(zero_art.get_root_public_key().x().as_ref()),
        );

        let leve_group_changes_output = zero_art.leave_group(new_secret_key)?;
        let leve_group_changes = leve_group_changes_output.get_branch_change().clone();

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::LeaveGroup(postcard::to_allocvec(
                    &leve_group_changes,
                )?)),
            }),
            protected_payload: Self::new_nonce(),
        };

        let mut buf = BytesMut::new();
        tbs_frame.encode(&mut buf)?;
        let associated_data = &Sha3_256::digest(&buf).to_vec();

        let mut proof_bytes = Vec::new();
        let proof = leve_group_changes_output.prove(associated_data, None)?;
        proof.serialize_compressed(&mut proof_bytes)?;

        let leave_result = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        if let Some(status_code) = status_check {
            assert_eq!(
                leave_result.0.status(),
                status_code,
                "Check if status code is correct: get {}, while waiting for {}.",
                leave_result.0.status(),
                status_code
            );
        }

        debug!(
            "LeaveGroup apply debug data:\n\tnew epoch: {}\n\tNew TK: {:#?}...",
            self.epoch + 1,
            stringify_option(leve_group_changes.public_keys.first()),
            // stringify_option(&zero_art.get_upstream_art().get_root_public_key().x()),
        );

        self.art = zero_art;
        self.epoch += 1;

        Ok(leave_result)
    }

    pub async fn get_messages(&self, limit: i64, skip: i64) -> eyre::Result<SpFrames> {
        let sk = self.art.get_base_art().get_root_secret_key();
        let pk = CortadoAffine::generator().mul(sk).into_affine();

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

    pub async fn get_challenge(&self) -> reqwest::Result<Vec<u8>> {
        let mut serialized_public_key = Vec::new();
        self.art
            .get_base_art()
            .get_leaf_public_key()
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
    ) -> eyre::Result<PublicArt<CortadoAffine>> {
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

        let sk = secret_key_to_use.unwrap_or(self.art.get_base_art().get_leaf_secret_key());
        // let sk = match secret_key_to_use {
        //     Some(secret_key) => secret_key,
        //     None => self.art.secret_key,
        // };

        let pk = CortadoAffine::generator().mul(sk).into_affine();

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

        let received_art =
            postcard::from_bytes(&get_art_response.json::<GetARTResponse>().await?.art)
                .map_err(ArtError::from)?;

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

        let pk = vec![self.art.get_base_art().get_leaf_public_key()];
        let signature = sign(
            &vec![self.art.get_base_art().get_leaf_secret_key()],
            &pk,
            msg,
        )
        .unwrap();
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
        // let pk = self.art.public_key_of(&sk);
        let pk = CortadoAffine::generator().mul(sk).into_affine();

        let signature = sign(&vec![sk], &vec![pk], msg)?;
        let verification_result = verify(&signature, &vec![pk], msg);
        assert!(verification_result.is_ok(), "Failed to verify signature");

        Ok(signature)
    }

    fn prove_and_check_schnorr_signature(
        &self,
        secret_keys: Vec<Fr>,
        tbs_frame: &FrameTbs,
    ) -> eyre::Result<Vec<u8>> {
        let public_keys = secret_keys
            .iter()
            .map(|sk| CortadoAffine::generator().mul(sk).into_affine())
            .collect::<Vec<_>>();
        let msg = Sha3_256::digest(&tbs_frame.encode_to_vec());
        let signature = sign(&secret_keys, &public_keys, &msg)?;
        let verification_result = verify(&signature, &public_keys, &msg);
        assert!(verification_result.is_ok());

        Ok(signature)
    }

    // fn prove_and_check_art_update<'a, R>(
    //     &self,
    //     zero_art: &'a PrivateZeroArt<'a, R>,
    //     secret_key: Fr,
    //     artefacts: &ProverArtefacts<CortadoAffine>,
    //     tbs_frame: &FrameTbs,
    //     changes: &BranchChanges<CortadoAffine>,
    // ) -> eyre::Result<Vec<u8>>
    // where
    //     R: Rng + ?Sized,
    // {
    //     let mut buf = BytesMut::new();
    //     tbs_frame.encode(&mut buf)?;
    //     let associated_data = &*Sha3_256::digest(&buf).to_vec();
    //
    //     let blindings: Vec<_> = (0..=artefacts.co_path.len())
    //         .map(|_| Scalar::random(&mut thread_rng()))
    //         .collect();
    //
    //     let public_key = CortadoAffine::generator().mul(secret_key).into_affine();
    //
    //     let proof = art_prove(
    //         Self::get_pedersen_basis(),
    //         associated_data,
    //         vec![public_key],
    //         artefacts.path.clone(),
    //         artefacts.co_path.clone(),
    //         artefacts.secrets.clone(),
    //         vec![secret_key],
    //         blindings,
    //     )?;
    //
    //     let verification_result = art_verify(
    //         Self::get_pedersen_basis(),
    //         associated_data,
    //         vec![public_key],
    //         changes.public_keys.iter().rev().copied().collect(),
    //         get_co_path_values(&test_art, &changes.node_index)?,
    //         proof.clone(),
    //     )
    //     .is_ok();
    //
    //     assert!(verification_result);
    //
    //     let mut proof_bytes = Vec::new();
    //     proof.serialize_compressed(&mut proof_bytes)?;
    //
    //     Ok(proof_bytes)
    // }

    pub fn unwrap_operation(frame: SpFrame) -> Operation {
        frame
            .frame
            .unwrap()
            .frame
            .unwrap()
            .group_operation
            .unwrap()
            .operation
            .unwrap()
    }

    pub async fn get_frames(
        &self,
        limit: i64,
        skip: i64,
        epoch: Option<u64>,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<SpFrames> {
        let tk = self.art.get_base_art().get_root_secret_key();
        let pk = self.art.get_base_art().get_root().get_public_key();

        let mut msg = Vec::new();
        let nonce = (0..DEFAULT_NONCE_LENGTH)
            .map(|_| rand::random::<u8>())
            .collect::<Vec<u8>>();
        msg.extend_from_slice(self.chat_uuid.as_bytes());
        msg.extend(&nonce);

        let msg = Sha3_256::digest(&msg).to_vec();
        debug!("Using root Pk {} for verification.", pk);

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

        if let Some(status) = status_check {
            assert_eq!(changes_response.status(), status);
        }

        let buf = BytesMut::from(&*changes_response.bytes().await?);
        let sp_frames = SpFrames::decode(buf)?;

        Ok(sp_frames)
    }

    pub async fn get_changes(
        &self,
        limit: i64,
        skip: i64,
        epoch: Option<u64>,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<Vec<BranchChange<CortadoAffine>>> {
        let sp_frames = self
            .get_frames(limit, skip, epoch, status_check)
            .await?
            .sp_frames;

        let mut changes = Vec::with_capacity(sp_frames.len());
        for sp_frame in sp_frames {
            let Some(frame) = sp_frame.frame else {
                continue;
            };

            if let Some(frame_change) = extract_branch_changes(&frame)? {
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
