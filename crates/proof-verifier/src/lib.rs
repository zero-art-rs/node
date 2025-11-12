use ark_ed25519::EdwardsAffine as Ed25519Affine;
use bulletproofs::PedersenGens;
use cortado::{ALT_GENERATOR_X, ALT_GENERATOR_Y, CortadoAffine};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use types::callback_wrappers::{
    ProofVerifierMessage, ProofVerifierMessageWrapper, ProofVerifierResult,
};
use zkp::ark_ec::AffineRepr;
use zkp::toolbox::{cross_dleq::PedersenBasis, dalek_ark::ristretto255_to_ark};
use zrt_art::changes::VerifiableChange;
use zrt_crypto::schnorr;
use zrt_zk::art::art_verify;

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
                change,
                art,
                associated_data,
                eligibility_requirement,
                proof,
            } => match change.verify(&art, &associated_data, eligibility_requirement, &proof) {
                Ok(_) => Ok(ProofVerifierResult::ArtUpdate { verdict: true }),
                Err(_) => Ok(ProofVerifierResult::ArtUpdate { verdict: false }),
            },
            ProofVerifierMessage::ArtAggregation {
                change,
                art,
                associated_data,
                eligibility_requirement,
                proof,
            } => match change.verify(&art, &associated_data, eligibility_requirement, &proof) {
                Ok(_) => Ok(ProofVerifierResult::ArtAggregation { verdict: true }),
                Err(_) => Ok(ProofVerifierResult::ArtAggregation { verdict: false }),
            },
            ProofVerifierMessage::SchnorrSignature {
                signature,
                public_keys,
                msg,
            } => {
                self.verify_schnorr_signature(signature.as_slice(), &public_keys, msg.as_slice())
                    .await
            }
        };

        let eyre_result = result.map_err(|err| eyre::eyre!("{}", err));

        if callback.send(eyre_result).is_err() {
            error!("Failed to send response: receiver dropped");
        }

        Ok(())
    }

    async fn verify_schnorr_signature(
        &self,
        signature: &[u8],
        public_keys: &Vec<CortadoAffine>,
        msg: &[u8],
    ) -> eyre::Result<ProofVerifierResult> {
        match schnorr::verify(signature, public_keys, msg) {
            Ok(_) => Ok(ProofVerifierResult::SchnorrSignature { verdict: true }),
            Err(e) => {
                warn!("Failed to verify schnorr signature: {}", e);
                Ok(ProofVerifierResult::SchnorrSignature { verdict: false })
            }
        }
    }
}
