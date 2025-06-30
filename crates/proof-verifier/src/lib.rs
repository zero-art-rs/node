use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use types::callback_wrappers::{
    ProofVerifierMessage, ProofVerifierMessageWrapper, ProofVerifierResult,
};

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
            ProofVerifierMessage::AddMember { proof } => self.verify_add_member_proof(proof).await,
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

    async fn verify_add_member_proof(&self, _proof: Vec<u8>) -> eyre::Result<ProofVerifierResult> {
        info!("Verifying add member proof");
        Ok(ProofVerifierResult::AddMember { verdict: true })
    }

    async fn verify_modify_art_proof(&self, _proof: Vec<u8>) -> eyre::Result<ProofVerifierResult> {
        info!("Verifying modify art proof");
        Ok(ProofVerifierResult::ModifyArt { verdict: true })
    }
}
