use crate::utils::{ArrayLessPrinter, CentrifugoTokenResponse, stringify_option};
use crate::{BACKEND_URL, DEFAULT_NONCE_LENGTH};
use ark_ec::{AffineRepr, CurveGroup};
use ark_ed25519::EdwardsAffine as Ed25519Affine;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
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
use tracing::{debug, error, trace, warn};
use types::protos::SpFrame;
use types::{
    DEFAULT_LIMIT, DEFAULT_SKIP,
    art_schemas::{ChallengeResponse, GetARTQuery, GetARTResponse},
    centrifugo_schemas::AuthRequest,
    messenger_schemas::GetMessageQuery,
    protos::{Frame, FrameTbs, GroupOperation, SpFrames, group_operation::Operation},
    utils,
    utils::extract_branch_changes,
};
use uuid::Uuid;
use zkp::rand::thread_rng;
use zkp::toolbox::{cross_dleq::PedersenBasis, dalek_ark::ristretto255_to_ark};
use zrt_art::art::{AggregationContext, ArtAdvancedOps, PrivateArt, PublicArt};
use zrt_art::art_node::{LeafStatus, TreeMethods};
use zrt_art::changes::ApplicableChange;
use zrt_art::changes::aggregations::{AggregatedChange, PrivateAggregatedChange};
use zrt_art::changes::branch_change::{BranchChange, BranchChangeType, PrivateBranchChange};
use zrt_art::errors::ArtError;
use zrt_art::node_index::{Direction, NodeIndex};
use zrt_crypto::schnorr::{sign, verify};
use zrt_zk::aggregated_art::ProverAggregationTree;
use zrt_zk::engine::{ZeroArtProverEngine, ZeroArtVerifierEngine};
use zrt_zk::{EligibilityArtefact, EligibilityRequirement, art::ArtProof};

#[derive(Clone)]
pub struct UserTestModel {
    pub client: reqwest::Client,
    pub art: PrivateArt<CortadoAffine>,
    pub initial_secrets: Vec<Fr>,
    pub chat_uuid: Uuid,
    pub epoch: u64,
    pub sequence_number: u64,
    pub owner_id_key: Option<Fr>,
    pub user_name: String,
    pub prover_engine: ZeroArtProverEngine,
    pub verifier_engine: ZeroArtVerifierEngine,
}

#[derive(Debug, Clone, thiserror::Error)]
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

fn default_user_name(index: usize) -> String {
    format!("User-{}", index)
}

impl UserTestModel {
    pub async fn new(size: u64) -> (Self, BytesMut) {
        let seed = 0;
        let mut rng = StdRng::seed_from_u64(seed);

        let secrets = (0..size).map(|_| Fr::rand(&mut rng)).collect::<Vec<_>>();
        debug!("Group secrets: {:#?}", secrets);
        let owner_id_key = Fr::rand(&mut rng);
        let art = PrivateArt::setup(&secrets).unwrap();
        let public_art = art.public_art().clone();

        let id = Uuid::now_v7();

        let user = Self {
            client: reqwest::Client::new(),
            art,
            initial_secrets: secrets,
            chat_uuid: id,
            epoch: 0, // init request is already one epoch
            sequence_number: 0,
            owner_id_key: Some(owner_id_key),
            user_name: default_user_name(0),
            prover_engine: Default::default(),
            verifier_engine: Default::default(),
        };

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
            self.art.root().path_to_leaf_with(
                CortadoAffine::generator()
                    .mul(self.initial_secrets[member_id])
                    .into_affine(),
            )?,
        ))
    }

    /// Clone this uses, and change this user secret key to the different one
    pub fn derive_new(&self, index: usize) -> Result<Self, ArtError> {
        let art = PrivateArt::new(self.art.public_art().clone(), self.initial_secrets[index])?;

        Ok(Self {
            client: reqwest::Client::new(),
            art,
            initial_secrets: self.initial_secrets.clone(),
            chat_uuid: self.chat_uuid,
            epoch: self.epoch,
            sequence_number: self.sequence_number,
            owner_id_key: None,
            user_name: default_user_name(index),
            prover_engine: Default::default(),
            verifier_engine: Default::default(),
        })
    }

    /// Clone this uses, and change this user secret key to the different one
    pub fn derive_new_with_sk(&self, user_index_name: usize, sk: Fr) -> Result<Self, ArtError> {
        let mut public_art = self.art.public_art().clone();
        public_art.commit().unwrap();
        let art = PrivateArt::new(public_art, sk)?;

        Ok(Self {
            client: reqwest::Client::new(),
            art,
            initial_secrets: self.initial_secrets.clone(),
            chat_uuid: self.chat_uuid,
            epoch: self.epoch,
            sequence_number: self.sequence_number,
            owner_id_key: None,
            user_name: default_user_name(user_index_name),
            prover_engine: Default::default(),
            verifier_engine: Default::default(),
        })
    }

    pub fn commit_epoch(&mut self) {
        self.art.commit().unwrap();
        self.epoch += 1;
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

    pub async fn create_key_update_frame(
        &mut self,
        payload: Option<Vec<u8>>,
    ) -> eyre::Result<(Frame, PrivateBranchChange<CortadoAffine>)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let new_secret_key = Fr::rand(&mut rng);

        let (_, branch_change, prover_branch) = self.art.update_key(new_secret_key)?;
        let private_branch_change = PrivateBranchChange::new(new_secret_key, branch_change.clone());

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

        let leaf_sk = self.art.secrets().preview().leaf();
        let leaf_pk = CortadoAffine::generator().mul(leaf_sk).into_affine();
        let prover_eligibility = EligibilityArtefact::Member((leaf_sk, leaf_pk));

        debug!(
            "UpdateKey creation debug data ({}):\
            \n\tnew epoch: {}\
            \n\tOld Tk: {:#?}...\
            \n\tOld commited Tk: {:#?}...\
            \n\tassociated_data: {:?}\
            \n\tprover_eligibility: {:?}",
            self.user_name,
            self.epoch + 1,
            stringify_option(self.art.root_public_key().x().as_ref()),
            stringify_option(self.art.preview().root().public_key().x().as_ref()),
            associated_data,
            prover_eligibility,
        );

        let mut proof_bytes = Vec::new();
        self.prover_engine
            .new_context(prover_eligibility)
            .for_branch(&prover_branch)
            .with_associated_data(&associated_data)
            .prove(&mut thread_rng())?
            .serialize_compressed(&mut proof_bytes)?;

        Ok((
            Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            },
            private_branch_change,
        ))
    }

    pub async fn send_frame_with_status_check(
        &mut self,
        frame: Frame,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let update_key_response = self.send_frame(frame).await?;

        if let Some(status_check) = status_check {
            if update_key_response.0.status() != status_check {
                Err(UserTestModelError::from((
                    update_key_response.0.status(),
                    status_check,
                )))?;
            }
        }

        Ok(update_key_response)
    }

    pub async fn update_key(
        &mut self,
        payload: Option<Vec<u8>>,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let (frame, change) = self.create_key_update_frame(payload).await?;

        let response = self
            .send_frame_with_status_check(frame, status_check)
            .await?;

        self.art.commit()?;
        self.epoch += 1;
        change.apply(&mut self.art).unwrap();

        Ok(response)
    }

    pub fn process_frame_tbs(&mut self, frame_tbs: &FrameTbs) {
        if let Some(operation) = &frame_tbs.group_operation {
            if let Some(inner_operation) = &operation.operation {
                // debug!("inner_operation: {:?}", inner_operation);
                match inner_operation {
                    Operation::KeyUpdate(change)
                    | Operation::AddMember(change)
                    | Operation::RemoveMember(change)
                    | Operation::LeaveGroup(change) => {
                        let branch_change = utils::decode_branch_change(change).unwrap();
                        if !matches!(branch_change.change_type, BranchChangeType::AddMember)
                            && self.art.node_index().eq(&branch_change.node_index)
                        {
                            trace!(
                                "Skip own operation ({}): {:?}",
                                self.user_name, inner_operation
                            );
                            return;
                        }

                        trace!(
                            "Apply operation ({}): {:?}",
                            self.user_name, inner_operation
                        );
                        branch_change.apply(&mut self.art).inspect_err(|err| {
                            error!("{} failed to apply operation. Error: {}. Change: {:?}. ART root:\n{}", self.user_name, err, branch_change, self.art.root());
                        }).unwrap();
                    }
                    Operation::Aggregated(change) => {
                        let branch_change = utils::decode_aggregated_change(change).unwrap();
                        branch_change.apply(&mut self.art).unwrap();
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn process_sp_frames(
        &mut self,
        sp_frames: &Vec<SpFrame>,
        max_epoch: Option<u64>,
    ) -> eyre::Result<bool> {
        for sp_frame in sp_frames {
            self.sequence_number += 1;

            let Some(frame) = &sp_frame.frame else {
                continue;
            };
            // let proof = ArtProof::deserialize_compressed(&*frame.proof);

            let Some(frame_tbs) = &frame.frame else {
                continue;
            };
            // let associated_data = Sha3_256::digest(frame_tbs.encode_to_vec()).to_vec();

            if let Some(max_epoch) = max_epoch {
                if frame_tbs.epoch >= max_epoch {
                    return Ok(false);
                }
            }

            if frame_tbs.epoch < self.epoch {
                warn!(
                    "{} received frame for the previous epoch: {:?}",
                    self.user_name,
                    sp_frame
                        .frame
                        .clone()
                        .and_then(|frame| frame.frame)
                        .and_then(|frame| frame.group_operation)
                        .and_then(|group_operation| group_operation.operation)
                );
                if sp_frame.seq_num > self.sequence_number {
                    self.sequence_number = sp_frame.seq_num;
                }
                continue;
            }

            if frame_tbs.epoch == self.epoch + 1 {
                self.art.commit().unwrap();
                self.epoch += 1;
            }

            trace!(
                "Self.epoch: {}, process frame_tbs: {:?}",
                self.epoch, frame_tbs
            );
            self.process_frame_tbs(frame_tbs);
        }

        Ok(true)
    }

    pub async fn poll(&mut self, max_epoch: Option<u64>) -> eyre::Result<()> {
        let mut skip = 0;
        let mut sp_frames = self
            .get_messages(DEFAULT_LIMIT, skip)
            .await
            .unwrap()
            .sp_frames;

        while !sp_frames.is_empty() {
            let continue_marker = self.process_sp_frames(&sp_frames, max_epoch).unwrap();
            if !continue_marker {
                break;
            }

            if sp_frames.len() < DEFAULT_LIMIT as usize {
                break;
            }

            skip += DEFAULT_LIMIT;
            sp_frames = self
                .get_messages(DEFAULT_LIMIT, skip)
                .await
                .unwrap()
                .sp_frames;
        }

        Ok(())
    }

    pub async fn send_aggregation(
        &mut self,
        agg: &AggregationContext<PrivateArt<CortadoAffine>, CortadoAffine>,
        new_sk: Option<Fr>,
        // mut zero_art: PrivateArt<CortadoAffine>,
        payload: Option<Vec<u8>>,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        debug!(
            "SendAggregation debug data:\n\
            \tepoch: {}\n\
            \tOld TK: {:#?}\n\
            \tcommited TK: {:#?}",
            self.epoch + 1,
            self.art.root().data().public_key(),
            agg.operation_tree().root().data().public_key(),
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

        let leaf_sk = self.art.secrets().preview().leaf();
        let leaf_pk = CortadoAffine::generator().mul(leaf_sk).into_affine();
        let mut proof_bytes = Vec::new();
        let prover_eligibility = EligibilityArtefact::Owner((leaf_sk, leaf_pk));
        self.prover_engine
            .new_context(prover_eligibility)
            .for_aggregation(&ProverAggregationTree::try_from(agg)?)
            .with_associated_data(&associated_data)
            .prove(&mut thread_rng())?
            .serialize_compressed(&mut proof_bytes)?;

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

        debug!(
            "SendAggregation comit debug data:\n\
            \tepoch: {}\n\
            \tNew TK: {:#?}",
            self.epoch + 1,
            stringify_option(agg.operation_tree().root_public_key().x().as_ref())
        );

        self.art.commit()?;
        if let Some(new_sk) = new_sk {
            let private_change = PrivateAggregatedChange::new(new_sk, aggregation_change);
            private_change.apply(&mut self.art).unwrap();
        } else {
            aggregation_change.apply(&mut self.art).unwrap();
        }
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
            self.art.root().data().public_key(),
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
        let tk = self.art.secrets().preview().root();
        let pk = vec![self.art.preview().root().public_key()];

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
        new_user_secret_key: Fr,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        // let mut rng = StdRng::seed_from_u64(rand::random());

        // let new_user_secret_key = Fr::rand(&mut rng);
        let (_, append_user_changes, prover_branch) = self
            .art
            .add_member(new_user_secret_key)
            .expect("Expected that add_member(...) works correctly");

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

        let leaf_sk = self.art.secrets().preview().leaf();
        let leaf_pk = CortadoAffine::generator().mul(leaf_sk).into_affine();
        let prover_eligibility = EligibilityArtefact::Owner((leaf_sk, leaf_pk));
        let proof = self
            .prover_engine
            .new_context(prover_eligibility)
            .for_branch(&prover_branch)
            .with_associated_data(&associated_data)
            .prove(&mut thread_rng())?;
        proof.serialize_compressed(&mut proof_bytes)?;

        let eligibility_requirement = EligibilityRequirement::Previleged((leaf_pk, vec![]));
        debug!("compute verification_branch ...");
        let verification_branch = self
            .art
            .preview()
            .verification_branch(&append_user_changes)?;
        debug!("computed verification_branch");
        self.verifier_engine
            .new_context(eligibility_requirement.clone())
            .with_associated_data(&associated_data)
            .for_branch(&verification_branch)
            .verify(&proof)
            .unwrap();

        debug!(
            "AddMember creation debug data:\
            \n\tnew epoch: {}\
            \n\tOld Tk: {:#?}...\
            \n\tOld commited Tk: {:#?}...\
            \n\tassociated_data: {:?}\
            \n\teligibility_requirement: {:?}",
            self.epoch + 1,
            stringify_option(self.art.root_public_key().x().as_ref()),
            stringify_option(self.art.preview().root().public_key().x().as_ref()),
            associated_data,
            eligibility_requirement,
        );

        let (request_response, request_bytes) = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        if let Some(status_code) = status_check {
            assert_eq!(request_response.status(), status_code);
        }

        self.art.commit()?;
        append_user_changes.apply(&mut self.art)?;

        debug!(
            "AddMember apply debug data:\n\tnew epoch: {}\n\tNew TK: {:#?}...",
            self.epoch + 1,
            stringify_option(self.art.root_public_key().x().as_ref()),
        );

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

        // let mut art_clone = self.art.clone();
        let (_, remove_user_changes, prover_branch) = self
            .art
            .remove_member(&user_to_remove_index, temporary_secret_key)?;

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
        let node_is_active = matches!(
            self.art.preview().node_at(user_to_remove)?.status(),
            Some(LeafStatus::Active)
        );
        let prover_eligibility = if node_is_active {
            EligibilityArtefact::Owner((self.art.leaf_secret_key(), self.art.leaf_public_key()))
        } else {
            EligibilityArtefact::Member((
                self.art.secrets().preview().root(),
                self.art.preview().root().public_key(),
            ))
        };
        self.prover_engine
            .new_context(prover_eligibility.clone())
            .for_branch(&prover_branch)
            .with_associated_data(&associated_data)
            .prove(&mut thread_rng())?
            .serialize_compressed(&mut proof_bytes)?;

        debug!(
            "MakeBlank creation debug data ({}):\n\t\
            epoch: {} -> {}\n\t\
            root pk: {}\n\t\
            preview root pk: {}\n\t\
            temporary_secret_key: {}\n\t\
            target_node_path: {:?}\n\t\
            prover_eligibility: {:?}\n\t\
            associated_data: {:?}",
            self.user_name,
            self.epoch,
            self.epoch + 1,
            stringify_option(self.art.root_public_key().x().as_ref()),
            stringify_option(self.art.preview().root().public_key().x().as_ref()),
            stringify_option(Some(&temporary_secret_key)),
            user_to_remove,
            prover_eligibility,
            associated_data,
        );

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

        self.art.commit()?;
        self.epoch += 1;
        remove_user_changes.apply(&mut self.art)?;
        debug!(
            "RemoveMember apply debug data:\n\tnew epoch: {}\n\tNew TK from change: {:#?}...",
            self.epoch,
            stringify_option(self.art.root_public_key().x().as_ref()),
        );

        Ok(make_blank_result)
    }

    pub async fn leave_group(
        &mut self,
        status_check: Option<StatusCode>,
    ) -> eyre::Result<(reqwest::Response, BytesMut)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let new_secret_key = Fr::rand(&mut rng);
        let (_, leve_group_changes, prover_branch) = self.art.leave_group(new_secret_key)?;

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
        let prover_eligibility =
            EligibilityArtefact::Member((self.art.leaf_secret_key(), self.art.leaf_public_key()));
        self.prover_engine
            .new_context(prover_eligibility.clone())
            .for_branch(&prover_branch)
            .with_associated_data(&associated_data)
            .prove(&mut thread_rng())?
            .serialize_compressed(&mut proof_bytes)?;

        debug!(
            "LeaveGroup creation debug data ({}):\n\t\
            current epoch: {} -> {}\n\t\
            Root pk: {:#?}...\n\t\
            Preview root pk: {:#?}...\n\t\
            prover_eligibility: {:?}\n\t\
            associated_data: {:?}",
            self.user_name,
            self.epoch,
            self.epoch + 1,
            stringify_option(self.art.root_public_key().x().as_ref()),
            stringify_option(self.art.preview().root().public_key().x().as_ref()),
            prover_eligibility,
            associated_data,
        );

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

        self.epoch += 1;

        Ok(leave_result)
    }

    pub async fn get_messages(&self, limit: i64, skip: i64) -> eyre::Result<SpFrames> {
        let sk = self.art.root_secret_key();
        let pk = CortadoAffine::generator().mul(sk).into_affine();

        let nonce = Self::new_nonce();

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_uuid.as_bytes());
        msg.extend(&nonce);

        let msg = Sha3_256::digest(&msg).to_vec();

        let signature = sign(&vec![sk], &vec![pk], &msg)?;
        let verification_result = verify(&signature, &vec![pk], &msg);
        assert!(verification_result.is_ok());

        debug!(
            "GetMessages polling debug data ({}):\
            \n\tnew epoch: {}\
            \n\tOld Tk: {:#?}...\
            \n\tassociated_data: {:?}\
            \n\teligibility_requirement: ({:?}..., {:?}...)",
            self.user_name,
            self.epoch + 1,
            stringify_option(self.art.root_public_key().x().as_ref()),
            msg,
            stringify_option(Some(&sk)),
            stringify_option(pk.x().as_ref()),
        );

        let get_messages_response = self
            .client
            .get(format!(
                "{}/{}/{}/{}",
                BACKEND_URL, "v1/group", self.chat_uuid, "frames"
            ))
            .query(&GetMessageQuery {
                message_sequence_number: Some(self.sequence_number as i64),
                limit,
                skip,
                signature,
                nonce,
                epoch: Some(self.epoch.saturating_sub(1)),
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
            .leaf_public_key()
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

        let sk = secret_key_to_use.unwrap_or(self.art.leaf_secret_key());
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

        let pk = vec![self.art.leaf_public_key()];
        let signature = sign(&vec![self.art.leaf_secret_key()], &pk, msg).unwrap();
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
        let tk = self.art.root_secret_key();
        let pk = self.art.root().data().public_key();

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
