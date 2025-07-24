use crate::domains::art::transport::http::GetInitialARTQuery;
use crate::verification::VerificationError;
use crate::{Container, as_base64};
use art::traits::ARTPublicAPI;
use art::types::NodeIndex;
use callbacks::callback;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetInitialARTHelper {
    chat_id: Uuid,
    #[serde(with = "as_base64")]
    nonce: Vec<u8>,
    index: u32,
    #[serde(with = "as_base64")]
    signature: Vec<u8>,
}

impl From<GetInitialARTQuery> for GetInitialARTHelper {
    fn from(query: GetInitialARTQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            nonce: query.nonce,
            index: query.index,
            signature: query.signature,
        }
    }
}

impl GetInitialARTHelper {
    pub async fn verify(&self, state: Arc<Container>) -> Result<(), VerificationError> {
        let mut initial_art_record = state.art_service.get_initial_art(&self.chat_id).await?;

        let leaf_node = initial_art_record
            .art
            .get_node(NodeIndex::Index(self.index))?;
        if !leaf_node.is_leaf() {
            return Err(VerificationError::InvalidProof);
        }

        let mut challenges_lock = state.challenges.lock().await;

        let challenge = match challenges_lock.get(&(self.chat_id, leaf_node.public_key)) {
            Some(challenge) => challenge.clone(),
            None => return Err(VerificationError::NoChallenge),
        };
        challenges_lock.remove_entry(&(self.chat_id, leaf_node.public_key));

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(&self.nonce);
        msg.extend(self.index.to_le_bytes());
        msg.extend(challenge);

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: self.signature.clone(),
            public_keys: vec![leaf_node.public_key],
            msg,
        };

        let verdict =
            match callback(&state.proof_verifier_sender, schnorr_signature_message).await? {
                ProofVerifierResult::SchnorrSignature { verdict } => {
                    info!(
                        "Remove used challenge for user {}, if chat {}",
                        leaf_node.public_key, self.chat_id
                    );

                    verdict
                }
                _ => return Err(VerificationError::InvalidResultMessage),
            };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }
}
