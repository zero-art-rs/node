use cortado::CortadoAffine;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use types::callback_wrappers::{
    ProofVerifierMessage, ProofVerifierMessageWrapper, ProofVerifierResult,
};
use zrt_crypto::schnorr;
use zrt_zk::engine::ZeroArtVerifierEngine;

pub mod verifier_engine;
pub use types::errors::VerificationError;

pub type ProofVerifierSender = mpsc::Sender<ProofVerifierMessageWrapper>;
pub type ProofVerifierReceiver = mpsc::Receiver<ProofVerifierMessageWrapper>;

pub struct ProofVerifier {
    listener: ProofVerifierReceiver,
    verifier_engine: ZeroArtVerifierEngine,
}

impl ProofVerifier {
    pub fn new(listener: ProofVerifierReceiver) -> Self {
        let verifier_engine = ZeroArtVerifierEngine::default();

        Self {
            listener,
            verifier_engine,
        }
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

        let result = match &event {
            ProofVerifierMessage::ArtUpdate {
                verification_branch,
                associated_data,
                eligibility_requirement,
                proof,
            } => {
                let result = self
                    .verifier_engine
                    .new_context(eligibility_requirement.clone())
                    .for_branch(&verification_branch)
                    .with_associated_data(&associated_data)
                    .verify(&proof)
                    .inspect_err(|err| error!(event = ?event, "Failed to verify: {err}"));

                match result {
                    Ok(_) => Ok(ProofVerifierResult::ArtUpdate { verdict: true }),
                    Err(_) => Ok(ProofVerifierResult::ArtUpdate { verdict: false }),
                }
            }
            ProofVerifierMessage::ArtAggregation {
                verification_tree,
                associated_data,
                eligibility_requirement,
                proof,
            } => {
                let result = self
                    .verifier_engine
                    .new_context(eligibility_requirement.clone())
                    .for_aggregation(&verification_tree)
                    .with_associated_data(&associated_data)
                    .verify(&proof)
                    .inspect_err(|err| error!(event = ?event, "Failed to verify: {err}"));

                match result {
                    Ok(_) => Ok(ProofVerifierResult::ArtAggregation { verdict: true }),
                    Err(_) => Ok(ProofVerifierResult::ArtAggregation { verdict: false }),
                }
            }
            ProofVerifierMessage::SchnorrSignature {
                signature,
                public_keys,
                msg,
            } => {
                self.verify_schnorr_signature(signature.as_slice(), &public_keys, msg.as_slice())
                    .await
                    .inspect_err(|err| error!(event = ?event, "Failed to verify: {err}",))
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
            Err(err) => {
                error!(
                    public_keys = ?public_keys,
                    "Failed to verify Schnorr signature: {err}"
                );
                Ok(ProofVerifierResult::SchnorrSignature { verdict: false })
            }
        }
    }
}
