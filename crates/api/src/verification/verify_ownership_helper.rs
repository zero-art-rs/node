use crate::domains::art::transport::http::DeleteChatQuery;
use crate::verification::VerificationError;
use crate::{Container, as_base64};
use callbacks::callback;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VerifyOwnershipHelper {
    chat_id: Uuid,
    #[serde(with = "as_base64")]
    nonce: Vec<u8>,
    #[serde(with = "as_base64")]
    signature: Vec<u8>,
}

impl From<DeleteChatQuery> for VerifyOwnershipHelper {
    fn from(query: DeleteChatQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            nonce: query.nonce,
            signature: query.signature,
        }
    }
}

impl VerifyOwnershipHelper {
    pub async fn verify(self, state: Arc<Container>) -> Result<(), VerificationError> {
        let art_record = state.art_service.get_art(&self.chat_id, None).await?;

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(self.nonce);

        let mut left_most_leaf = &art_record.art.root;
        while let Ok(node) = left_most_leaf.get_left() {
            left_most_leaf = node;
        }

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: self.signature,
            public_keys: vec![left_most_leaf.public_key],
            msg,
        };

        let ProofVerifierResult::SchnorrSignature { verdict } =
            callback(&state.proof_verifier_sender, schnorr_signature_message).await?
        else {
            return Err(VerificationError::InvalidResultMessage);
        };

        if !verdict {
            return Err(VerificationError::InvalidProof);
        }

        Ok(())
    }
}
