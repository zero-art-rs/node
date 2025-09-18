use ark_ec::{AffineRepr, CurveGroup};
use ark_ed25519::EdwardsAffine as Ed25519Affine;
use ark_serialize::CanonicalSerialize;
use ark_std::{
    UniformRand,
    rand::prelude::StdRng,
    rand::{SeedableRng, thread_rng},
};
use art::types::{BranchChanges, Direction, NodeIndex, ProverArtefacts};
use art::{
    errors::ARTError,
    traits::{ARTPrivateAPI, ARTPrivateView, ARTPublicAPI},
    types::{PrivateART, PublicART},
};
use axum::body::Bytes;
use bulletproofs::PedersenGens;
use bytes::BytesMut;
use cortado::{CortadoAffine, Fr};
use crypto::schnorr::{sign, verify};
use curve25519_dalek::Scalar;
use prost::Message;
use reqwest::StatusCode;
use std::ops::Mul;
use axum_test::{TestResponse, TestServer};
use tracing::debug;
use tracing::field::debug;
use types::{
    art_schemas::*,
    centrifugo_schemas::*,
    messenger_schemas::GetMessageQuery,
    messenger_schemas::*,
    protos,
    protos::{Frame, FrameTbs, GroupOperation, SpFrames, group_operation::Operation},
    utils::extract_branch_changes,
};
use uuid::Uuid;
use zk::art::{art_prove, art_verify};
use zkp::toolbox::{cross_dleq::PedersenBasis, dalek_ark::ristretto255_to_ark};
use sha3::{Digest, Sha3_256};

const BACKEND_URL: &str = "http://localhost:8080";
const CENTRIFUGO_URL: &str = "http://localhost:8000";
// used for tests, which can be repeated
const TEST_REPEATS: usize = 4;
const DEFAULT_NONCE_LENGTH: u32 = 16; // 16 bytes
const DEFAULT_GROUP_SIZE: u64 = 10;

#[derive(Debug)]
pub(crate) struct UserTestModel<'a> {
    pub test_server: &'a TestServer,
    pub art: PrivateART<CortadoAffine>,
    pub initial_secrets: Vec<Fr>,
    pub chat_uuid: Uuid,
    pub epoch: u64,
    pub owner_id_key: Option<Fr>,
}

impl<'a> UserTestModel<'a> {
    pub async fn new(test_server: &'a TestServer, size: u64) -> (Self, BytesMut) {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let secrets = (0..size).map(|_| Fr::rand(&mut rng)).collect();
        let owner_id_key = Fr::rand(&mut rng);
        let (art, _) =
            PrivateART::new_art_from_secrets(&secrets, &CortadoAffine::generator()).unwrap();

        let id = Uuid::now_v7();

        let mut user = Self {
            test_server,
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
        response.assert_status(StatusCode::CREATED);

        (user, init_message)
    }

    pub fn index_of(&self, member_id: usize) -> Result<NodeIndex, ARTError> {
        Ok(NodeIndex::from(
            self.art.get_path_to_leaf(
                &self.art.public_key_of(
                    &self.initial_secrets[member_id],
                )
            )?
        ))
    }

    /// Clone this uses, and change this user secret key to the different one
    pub fn derive_new(&self, index: i64) -> Result<Self, ARTError> {
        let (mut art, _) =
            PrivateART::new_art_from_secrets(&self.initial_secrets, &CortadoAffine::generator())?;
        art.secret_key = self.initial_secrets[index as usize].clone();
        art.update_node_index()?;

        Ok(Self {
            test_server: self.test_server,
            art,
            initial_secrets: self.initial_secrets.clone(),
            chat_uuid: self.chat_uuid,
            epoch: self.epoch,
            owner_id_key: None,
        })
    }

    pub fn is_owner(&self) -> bool {
        match self.owner_id_key {
            Some(_) => true,
            None => false,
        }
    }

    async fn create_new_chat(
        &self,
        art: PublicART<CortadoAffine>,
        sk: Fr,
    ) -> eyre::Result<(TestResponse, BytesMut)> {
        let pk = art.public_key_of(&sk);
        let mut serialized_pk = Vec::new();
        pk.serialize_uncompressed(&mut serialized_pk)?;

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

        let msg = Sha3_256::digest(msg.to_vec());

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
    ) -> eyre::Result<(TestResponse, BytesMut)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        let secret_key = self.art.secret_key.clone();
        let new_secret_key = Fr::rand(&mut rng);

        let (_, key_update_changes, artefacts) = self.art.update_key(&new_secret_key)?;

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::KeyUpdate(key_update_changes.serialze()?)),
            }),
            protected_payload: payload.unwrap_or(vec![]),
        };

        let proof_bytes =
            self.prove_and_check_art_update(secret_key, &artefacts, &tbs_frame, &key_update_changes)?;

        let update_key_response = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        update_key_response.0.assert_status(StatusCode::OK);

        self.epoch += 1;
        Ok(update_key_response)
    }

    // add node to the art, and send updates to the chat
    pub async fn add_member(&mut self) -> eyre::Result<(TestResponse, BytesMut)> {
        let mut rng = StdRng::seed_from_u64(rand::random());

        // let old_tk = self.art.get_root_key()?.key;
        let old_tk = self.art.secret_key.clone();
        let new_user_secret_key = Fr::rand(&mut rng);
        let (_, append_user_changes, artefacts) = self.art.append_or_replace_node(&new_user_secret_key)?;

        let tbs_frame = FrameTbs {
            group_id: self.chat_uuid.to_string(),
            epoch: self.epoch + 1,
            nonce: vec![],
            group_operation: Some(GroupOperation {
                operation: Some(Operation::AddMember(append_user_changes.serialze()?)),
            }),
            protected_payload: vec![],
        };

        let proof_bytes =
            self.prove_and_check_art_update(old_tk, &artefacts, &tbs_frame, &append_user_changes)?;

        let add_member_response = self
            .send_frame(Frame {
                frame: Some(tbs_frame),
                proof: proof_bytes,
            })
            .await?;

        add_member_response.0.assert_status(StatusCode::OK);
        self.epoch += 1;

        Ok(add_member_response)
    }


    fn sign_and_check_signature(&self, msg: &[u8], sk: Fr) -> eyre::Result<Vec<u8>> {
        let pk = self.art.public_key_of(&sk);

        let signature = sign(&vec![sk], &vec![pk], &msg)?;
        let verification_result = verify(&signature, &vec![pk], msg);
        assert!(verification_result.is_ok(), "Failed to verify signature");

        Ok(signature)
    }

    fn prove_and_check_art_update(
        &self,
        secret_key: Fr,
        artefacts: &ProverArtefacts<CortadoAffine>,
        tbs_frame: &FrameTbs,
        changes: &BranchChanges<CortadoAffine>,
    ) -> eyre::Result<Vec<u8>> {
        let mut buf = BytesMut::new();
        tbs_frame.encode(&mut buf)?;
        let associated_data = &*Sha3_256::digest(&buf).to_vec();

        let blindings: Vec<_> = (0..artefacts.co_path.len() + 1)
            .map(|_| Scalar::random(&mut thread_rng()))
            .collect();

        let public_key = CortadoAffine::generator().mul(secret_key).into_affine();

        debug!("Using public_key.x: {} for proof creation.", &public_key.x);

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
            changes.public_keys.iter().rev().cloned().collect(),
            self.art.get_co_path_values(&changes.node_index)?,
            proof.clone(),
        )
        .is_ok();

        assert_eq!(verification_result, true);

        let mut proof_bytes = Vec::new();
        proof.serialize_uncompressed(&mut proof_bytes)?;

        Ok(proof_bytes)
    }

    // pub async fn get_changes(
    //     &self,
    //     limit: i64,
    //     skip: i64,
    //     epoch: Option<u64>,
    // ) -> eyre::Result<Vec<BranchChanges<CortadoAffine>>> {
    //     let tk = self.art.get_root_key()?.key;
    //     let pk = self.art.root.public_key;
    //
    //     let mut msg = Vec::new();
    //     let nonce = (0..DEFAULT_NONCE_LENGTH)
    //         .map(|_| rand::random::<u8>())
    //         .collect::<Vec<u8>>();
    //     msg.extend_from_slice(self.chat_uuid.as_bytes());
    //     msg.extend(&nonce);
    //
    //     debug!("Using {} for verification.", pk.x);
    //
    //     let signature = sign(&vec![tk], &vec![pk], &msg).unwrap();
    //
    //     assert!(verify(&signature, &vec![pk], &msg).is_ok());
    //
    //     let changes_response = self
    //         .client
    //         .get(format!(
    //             "{}/{}/{}/{}",
    //             BACKEND_URL, "v1/group", self.chat_uuid, "frames"
    //         ))
    //         .query(&GetMessageQuery {
    //             message_sequence_number: None,
    //             signature,
    //             limit,
    //             skip,
    //             nonce,
    //             epoch,
    //         })
    //         .send()
    //         .await?;
    //
    //     assert_eq!(changes_response.status(), StatusCode::ACCEPTED);
    //
    //     let buf = BytesMut::from(&*changes_response.bytes().await?);
    //     let sp_frames = SpFrames::decode(buf)?.sp_frames;
    //
    //     let mut changes = Vec::with_capacity(sp_frames.len());
    //     for sp_frame in sp_frames {
    //         let frame = match sp_frame.frame {
    //             Some(frame) => frame,
    //             None => continue,
    //         };
    //
    //         if let Some(frame_change) = extract_branch_changes(&frame)? {
    //             changes.push(frame_change);
    //         }
    //     }
    //
    //     Ok(changes)
    // }

    pub fn new_nonce() -> Vec<u8> {
        std::iter::repeat(rand::random::<u8>())
            .take(DEFAULT_NONCE_LENGTH as usize)
            .collect::<Vec<u8>>()
    }

    /// returns response from the server and the message, which was sent
    pub async fn send_frame(&self, frame: Frame) -> eyre::Result<(TestResponse, BytesMut)> {
        let mut buf = BytesMut::new();
        frame.encode(&mut buf).unwrap();

        Ok((
            self.test_server
                .post(&format!(
                    "{}/{}/{}/{}",
                    BACKEND_URL, "v1/group", self.chat_uuid, "frames"
                ))
                .bytes(Bytes::from(buf.clone()))
                .await,
            buf,
        ))
    }
}
