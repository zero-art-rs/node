use crate::domains::art::transport::http::{GetARTQuery, GetChangesQuery};
use crate::domains::messenger::transport::http::{
    DeleteMessageQuery, GetMessageQuery, SendMessageRequest,
};
use crate::errors::ApiError;
use crate::{Container, as_base64};
use callbacks::callback;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info};
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub enum KnowledgeVerificationType {
    VerifyCurrent,
    VerifyPrevious,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RootKnowledgeHelper {
    chat_uuid: Uuid,
    sequence_number: Option<i64>,
    #[serde(with = "as_base64")]
    nonce: Vec<u8>,
    #[serde(with = "as_base64")]
    signature: Vec<u8>,
    verification_type: KnowledgeVerificationType,
}

impl From<SendMessageRequest> for RootKnowledgeHelper {
    fn from(query: SendMessageRequest) -> Self {
        Self {
            chat_uuid: query.chat_id,
            sequence_number: None,
            nonce: query.nonce,
            signature: query.signature,
            verification_type: KnowledgeVerificationType::VerifyCurrent,
        }
    }
}

impl From<GetMessageQuery> for RootKnowledgeHelper {
    fn from(query: GetMessageQuery) -> Self {
        Self {
            chat_uuid: query.chat_id,
            sequence_number: query.sequence_number,
            nonce: query.nonce,
            signature: query.signature,
            verification_type: KnowledgeVerificationType::VerifyCurrent,
        }
    }
}

impl From<DeleteMessageQuery> for RootKnowledgeHelper {
    fn from(query: DeleteMessageQuery) -> Self {
        Self {
            chat_uuid: query.chat_id,
            sequence_number: query.sequence_number,
            nonce: query.nonce,
            signature: query.signature,
            verification_type: KnowledgeVerificationType::VerifyCurrent,
        }
    }
}

impl From<GetChangesQuery> for RootKnowledgeHelper {
    fn from(query: GetChangesQuery) -> Self {
        Self {
            chat_uuid: query.chat_id,
            sequence_number: query.sequence_number,
            nonce: query.nonce,
            signature: query.signature,
            verification_type: KnowledgeVerificationType::VerifyPrevious,
        }
    }
}

impl From<GetARTQuery> for RootKnowledgeHelper {
    fn from(query: GetARTQuery) -> Self {
        Self {
            chat_uuid: query.chat_id,
            sequence_number: query.sequence_number,
            nonce: query.nonce,
            signature: query.signature,
            verification_type: KnowledgeVerificationType::VerifyPrevious,
        }
    }
}

impl RootKnowledgeHelper {
    pub async fn verify(&self, state: Arc<Container>) -> Result<(), ApiError> {
        let art_record = match self.verification_type {
            KnowledgeVerificationType::VerifyCurrent => {
                info!("VerifyCurrent");
                state
                    .art_service
                    .get_art(&self.chat_uuid, self.sequence_number)
                    .await?
            }
            KnowledgeVerificationType::VerifyPrevious => {
                info!("VerifyPrevious");
                state
                    .art_service
                    .get_previous_art(&self.chat_uuid, self.sequence_number)
                    .await?
            }
        };

        let mut msg = Vec::new();
        msg.extend_from_slice(self.chat_uuid.as_bytes());
        msg.extend(&self.nonce);

        info!("root_pk: {}", art_record.art.root.public_key);

        let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
            signature: self.signature.clone(),
            public_keys: vec![art_record.art.root.public_key],
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
