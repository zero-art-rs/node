use crate::domains::art::transport::http::DeleteChatQuery;
use crate::errors::ApiError;
use crate::{Container, as_base64};
use art::traits::ARTPublicAPI;
use art::types::NodeIndex;
use callbacks::callback;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::sync::Arc;
use tracing::{error, info};
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

impl TryFrom<&[u8]> for VerifyOwnershipHelper {
    type Error = ApiError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match serde_urlencoded::from_bytes::<DeleteChatQuery>(value) {
            Ok(query) => Ok(Self::from(query)),
            Err(err) => Err(ApiError::BadRequest(err.to_string())),
        }
    }
}

impl VerifyOwnershipHelper {
    pub async fn verify(self, state: Arc<Container>) -> Result<(), ApiError> {
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

        match callback(&state.proof_verifier_sender, schnorr_signature_message).await {
            Ok(message) => {
                let ProofVerifierResult::SchnorrSignature { verdict } = message else {
                    return Err(ApiError::InternalServerError(
                        "Invalid message from proof verifier".to_string(),
                    ));
                };

                if !verdict {
                    return Err(ApiError::BadRequest("Invalid proof".to_string()));
                }
            }
            Err(e) => {
                error!("Failed to send message to proof verifier: {}", e);
                return Err(ApiError::InternalServerError(e.to_string()));
            }
        };

        Ok(())
    }
}
