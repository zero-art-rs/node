use ark_ed25519::EdwardsAffine as Ed25519Affine;
use ark_serialize::CanonicalDeserialize;
use bulletproofs::{BulletproofGens, PedersenGens};
use cortado::{self, CortadoAffine, FromScalar, Parameters, ToScalar};
use crypto::schnorr;
use tokio::sync::mpsc;
use tokio_util::bytes::Buf;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use types::callback_wrappers::{
    ProofVerifierMessage, ProofVerifierMessageWrapper, ProofVerifierResult,
};
use zk::art::{ARTProof, art_verify};
use zkp::ark_ec::AffineRepr;
use zkp::toolbox::{cross_dleq::PedersenBasis, dalek_ark::ristretto255_to_ark};

pub mod verifier_engine;

pub use types::errors::VerificationError;

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
            ProofVerifierMessage::ArtUpdate {
                associated_data,
                aux_public_keys,
                path,
                co_path,
                proof,
            } => {
                self.verify_art_update_proof(associated_data, aux_public_keys, path, co_path, proof)
                    .await
            }
            ProofVerifierMessage::SchnorrSignature {
                signature,
                public_keys,
                msg,
            } => {
                self.verify_schnorr_signature(signature.as_slice(), &public_keys, msg.as_slice())
                    .await
            }
        };

        let eyre_result = match result {
            Ok(proof_verifier_result) => Ok(proof_verifier_result),
            Err(proof_verifier_err) => Err(eyre::eyre!("{}", proof_verifier_err)),
        };

        if callback.send(eyre_result).is_err() {
            error!("Failed to send response: receiver dropped");
        }

        Ok(())
    }

    async fn verify_art_update_proof(
        &self,
        associated_data: Vec<u8>,
        aux_public_keys: Vec<CortadoAffine>,
        path: Vec<CortadoAffine>,
        co_path: Vec<CortadoAffine>,
        proof: Vec<u8>,
    ) -> eyre::Result<ProofVerifierResult> {
        debug!("Verify art update proof");

        let verification_result = art_verify(
            get_pedersen_basis(),
            associated_data.as_slice(),
            aux_public_keys,
            path,
            co_path,
            ARTProof::deserialize_uncompressed(proof.reader())?,
        );

        match verification_result {
            Ok(_) => Ok(ProofVerifierResult::ArtUpdate { verdict: true }),
            Err(e) => {
                error!("Failed to verify art update proof: {}", e);
                Ok(ProofVerifierResult::ArtUpdate { verdict: false })
            }
        }
    }

    async fn verify_schnorr_signature(
        &self,
        signature: &[u8],
        public_keys: &Vec<CortadoAffine>,
        msg: &[u8],
    ) -> eyre::Result<ProofVerifierResult> {
        debug!("Verifying schnorr signature");
        match schnorr::verify(signature, public_keys, msg) {
            Ok(_) => Ok(ProofVerifierResult::SchnorrSignature { verdict: true }),
            Err(e) => {
                warn!("Failed to verify schnorr signature: {}", e);
                Ok(ProofVerifierResult::SchnorrSignature { verdict: false })
            }
        }
    }
}

fn get_pedersen_basis() -> PedersenBasis<cortado::cortado::CortadoAffine, Ed25519Affine> {
    let g_1 = cortado::cortado::CortadoAffine::generator();
    let h_1 = cortado::cortado::CortadoAffine::new_unchecked(
        cortado::ALT_GENERATOR_X,
        cortado::ALT_GENERATOR_Y,
    );

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
