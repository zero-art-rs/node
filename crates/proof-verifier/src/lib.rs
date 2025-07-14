use ark_ed25519::EdwardsAffine as Ed25519Affine;
use ark_serialize::CanonicalDeserialize;
use bulletproofs::{BulletproofGens, PedersenGens};
use cortado::CortadoAffine;
use tokio::sync::mpsc;
use tokio_util::bytes::Buf;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use types::callback_wrappers::{
    ProofVerifierMessage, ProofVerifierMessageWrapper, ProofVerifierResult,
};
use zk::art::{ARTProof, art_verify};
use zkp::ark_ec::AffineRepr;
use zkp::toolbox::{cross_dleq::PedersenBasis, dalek_ark::ristretto255_to_ark};
use crypto::schnorr;

pub type ProofVerifierSender = mpsc::Sender<ProofVerifierMessageWrapper>;

pub type ProofVerifierReceiver = mpsc::Receiver<ProofVerifierMessageWrapper>;

#[derive(Debug)]
pub struct ProofVerifier {
    listener: ProofVerifierReceiver,
}

impl ProofVerifier {
    pub fn new(listener: ProofVerifierReceiver) -> Self {
        Self { listener }
    }

    pub async fn run(mut self, cancellation_token: CancellationToken) {
        loop {
            tokio::select! {
                event_opt = self.listener.recv() => {
                    let Some(event) = event_opt else {
                        info!("Event channel closed, stopping");
                        break;
                    };

                    if let Err(e) = self.handle_event(event).await {
                        error!("Failed to handle event: {}", e);
                    }
                }
                _ = cancellation_token.cancelled() => {
                    info!("Cancellation received, stopping");
                    break;
                }
            }
        }
    }

    async fn handle_event(&self, event: ProofVerifierMessageWrapper) -> eyre::Result<()> {
        let (event, callback) = event.inner_owned();

        let result = match event {
            ProofVerifierMessage::AddMember {
                proof,
                co_path,
                associated_data,
            } => {
                self.verify_add_member_proof(proof, co_path, associated_data)
                    .await
            }
            ProofVerifierMessage::ModifyArt { proof } => self.verify_modify_art_proof(proof).await,
        };

        let eyre_result = match result {
            Ok(proof_verifier_result) => Ok(proof_verifier_result),
            Err(proof_verifier_err) => Err(eyre::eyre!("{}", proof_verifier_err)),
        };

        if let Err(_) = callback.send(eyre_result) {
            error!("Failed to send response: receiver dropped");
        }

        Ok(())
    }

    async fn verify_add_member_proof(
        &self,
        proof: Vec<u8>,
        co_path: Vec<u8>,
        associated_data: Vec<u8>,
    ) -> eyre::Result<ProofVerifierResult> {
        info!("Verifying add member proof");
        let verification_result = art_verify(
            &get_bulletproof_gens(),
            get_pedersen_basis(),
            associated_data.as_slice(),
            Vec::<CortadoAffine>::deserialize_uncompressed(co_path.reader())?,
            ARTProof::deserialize_uncompressed(proof.reader())?,
        );

        match verification_result {
            Ok(_) => Ok(ProofVerifierResult::AddMember { verdict: true }),
            Err(e) => {
                info!("Failed to verify add_member_proof: {}", e);
                Ok(ProofVerifierResult::AddMember { verdict: false })
            }
        }
    }

    async fn verify_modify_art_proof(&self, _proof: Vec<u8>) -> eyre::Result<ProofVerifierResult> {
        info!("Verifying modify art proof");
        Ok(ProofVerifierResult::ModifyArt { verdict: true })
    }

    async fn verify_key_update_proof(
        &self,
        proof: Vec<u8>,
        co_path: Vec<u8>,
        associated_data: Vec<u8>,
    ) -> eyre::Result<ProofVerifierResult> {
        info!("Verifying key update proof");
        let verification_result = art_verify(
            &get_bulletproof_gens(),
            get_pedersen_basis(),
            associated_data.as_slice(),
            Vec::<CortadoAffine>::deserialize_uncompressed(co_path.reader())?,
            ARTProof::deserialize_uncompressed(proof.reader())?,
        );

        match verification_result {
            Ok(_) => Ok(ProofVerifierResult::AddMember { verdict: true }),
            Err(e) => {
                info!("Failed to verify add_member_proof: {}", e);
                Ok(ProofVerifierResult::AddMember { verdict: false })
            }
        }
    }

    async fn verify_init_group_proof(
        &self,
        proof: Vec<u8>,
        co_path: Vec<u8>,
        associated_data: Vec<u8>,
    ) -> eyre::Result<ProofVerifierResult> {
        info!("Verifying key update proof");
        let verification_result = art_verify(
            &get_bulletproof_gens(),
            get_pedersen_basis(),
            associated_data.as_slice(),
            Vec::<CortadoAffine>::deserialize_uncompressed(co_path.reader())?,
            ARTProof::deserialize_uncompressed(proof.reader())?,
        );

        match verification_result {
            Ok(_) => Ok(ProofVerifierResult::AddMember { verdict: true }),
            Err(e) => {
                info!("Failed to verify add_member_proof: {}", e);
                Ok(ProofVerifierResult::AddMember { verdict: false })
            }
        }
    }

    async fn verify_remove_member_proof(
        &self,
        proof: Vec<u8>,
        co_path: Vec<u8>,
        associated_data: Vec<u8>,
    ) -> eyre::Result<ProofVerifierResult> {
        info!("Verifying key update proof");
        let verification_result = art_verify(
            &get_bulletproof_gens(),
            get_pedersen_basis(),
            associated_data.as_slice(),
            Vec::<CortadoAffine>::deserialize_uncompressed(co_path.reader())?,
            ARTProof::deserialize_uncompressed(proof.reader())?,
        );

        match verification_result {
            Ok(_) => Ok(ProofVerifierResult::AddMember { verdict: true }),
            Err(e) => {
                info!("Failed to verify add_member_proof: {}", e);
                Ok(ProofVerifierResult::AddMember { verdict: false })
            }
        }
    }

    async fn verify_schnorr_signature (signature: &[u8], public_keys: &Vec<CortadoAffine>, msg: &[u8]) -> bool {
        info!("Verifying schnorr signature");
        match schnorr::verify(signature, public_keys, msg) {
            Ok(_) => true,
            Err(e) => {
                info!("Failed to verify schnorr_signature: {}", e);
                false
            }
        }
    }
}

fn get_pedersen_basis() -> PedersenBasis<CortadoAffine, Ed25519Affine> {
    let g_1 = CortadoAffine::generator();
    let h_1 = CortadoAffine::new_unchecked(cortado::ALT_GENERATOR_X, cortado::ALT_GENERATOR_Y);

    let gens = PedersenGens::default();
    PedersenBasis::<CortadoAffine, Ed25519Affine>::new(
        g_1,
        h_1,
        ristretto255_to_ark(gens.B).unwrap(),
        ristretto255_to_ark(gens.B_blinding).unwrap(),
    )
}

fn get_bulletproof_gens() -> BulletproofGens {
    BulletproofGens::new(2048, 1)
}
