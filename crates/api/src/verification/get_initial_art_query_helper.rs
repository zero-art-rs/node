use crate::domains::art::transport::http::GetInitialARTQuery;
use crate::errors::ApiError;
use crate::{Container, as_base64};
use art::traits::ARTPublicAPI;
use art::types::NodeIndex;
use callbacks::callback;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::sync::Arc;
use tracing::error;
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetInitialARTQueryHelper {
    chat_id: Uuid,
    #[serde(with = "as_base64")]
    nonce: Vec<u8>,
    index: u32,
    #[serde(with = "as_base64")]
    signature: Vec<u8>,
}

impl From<GetInitialARTQuery> for GetInitialARTQueryHelper {
    fn from(query: GetInitialARTQuery) -> Self {
        Self {
            chat_id: query.chat_id,
            nonce: query.nonce,
            index: query.index,
            signature: query.signature,
        }
    }
}

impl TryFrom<&[u8]> for GetInitialARTQueryHelper {
    type Error = ApiError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        match serde_json::from_slice::<GetInitialARTQuery>(value) {
            Ok(query) => Ok(Self::from(query)),
            Err(err) => Err(ApiError::BadRequest(err.to_string())),
        }
    }
}

impl GetInitialARTQueryHelper {
    pub async fn verify(self, state: Arc<Container>) -> Result<(), ApiError> {
        let mut initial_art_record = state
            .art_service
            .get_initial_art(&self.chat_id)
            .await
            .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

        let leaf_node = initial_art_record
            .art
            .get_node(NodeIndex::Index(self.index))?;
        if !leaf_node.is_leaf() {
            return Err(ApiError::BadRequest("The node isn't a leaf".to_string()));
        }

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_id.as_bytes());
        msg.extend(self.nonce);
        msg.extend(self.index.to_le_bytes());

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: self.signature,
            public_keys: vec![leaf_node.public_key],
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
