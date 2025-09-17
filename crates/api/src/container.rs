use crate::domains::{
    art::service::ARTService, centrifugo::service::CentrifugoService,
    messenger::service::MessengerService,
};
use ark_serialize::CanonicalDeserialize;
use art::types::BranchChangesType;
use axum::body::Bytes;
use axum::http::StatusCode;
use proof_verifier::ProofVerifierSender;
use prost::Message;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;
use types::errors::{ARTServiceError, ServiceError};
use types::protos::Frame;
use types::protos::group_operation::Operation;
use types::utils::decode_branch_changes;
use uuid::Uuid;

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub art_service: Arc<ARTService>,

    pub proof_verifier_sender: ProofVerifierSender,

    pub challenges: Arc<RwLock<HashSet<Vec<u8>>>>,
}

impl Container {
    pub async fn send_frame(&self, id: Uuid, body: Bytes) -> Result<StatusCode, ServiceError> {
        let frame = Frame::decode(body.clone())?;

        let tbs_frame = frame.frame.ok_or_else(|| ARTServiceError::InvalidInput)?;

        let operation = match tbs_frame.group_operation {
            None => None,
            Some(val) => val.operation,
        };

        // Decide, how to handle request
        let response = match operation {
            Some(Operation::Init(public_art)) => {
                self.art_service
                    .init_group(id, public_art, false, tbs_frame.nonce)
                    .await?;

                StatusCode::CREATED
            }
            Some(Operation::AddMember(changes))
            | Some(Operation::RemoveMember(changes))
            | Some(Operation::KeyUpdate(changes)) => {
                self.update_art(id, changes, tbs_frame.epoch).await?
            }
            Some(Operation::DropGroup(_)) => {
                return Ok(self
                    .art_service
                    .delete_chat(&id)
                    .await
                    .map(|_| StatusCode::NO_CONTENT)?);
            }
            Some(Operation::LeaveGroup(index)) => self.handle_self_removal(id, index).await?,
            None => StatusCode::OK,
        };

        self.messenger_service
            .send_message(body.to_vec(), &id, tbs_frame.epoch as i64, false)
            .await?;

        Ok(response)
    }

    pub async fn handle_self_removal(&self, id: Uuid, index: u64) -> Result<StatusCode, ServiceError> {
        self.art_service.mark_as_removed(id, index).await?;

        Ok(StatusCode::OK)
    }

    pub async fn update_art(
        &self,
        id: Uuid,
        branch_changes_bytes: Vec<u8>,
        new_epoch: u64,
    ) -> Result<StatusCode, ServiceError> {
        // self.start_updating(chat_id).await?;

        let branch_changes =
            decode_branch_changes(&branch_changes_bytes).map_err(ARTServiceError::from)?;

        let current_epoch = self.art_service.get_current_epoch(id).await?;

        match new_epoch {
            e if e == current_epoch => {
                // resolve merge conflict
                self.art_service.merge_change(id, branch_changes.clone(), new_epoch).await?;
            }
            e if e == current_epoch + 1 => {
                // update art and increment epoch
                self.art_service.update_art(id, &branch_changes).await?;
            }
            _ => return Err(ARTServiceError::InvalidInput.into()),
        }

        // self.stop_updating(chat_id).await;

        match branch_changes.change_type {
            BranchChangesType::UpdateKey => Ok(StatusCode::OK),
            BranchChangesType::AppendNode => Ok(StatusCode::OK),
            BranchChangesType::MakeBlank => Ok(StatusCode::NO_CONTENT),
            _ => Ok(StatusCode::NOT_IMPLEMENTED),
        }
    }
}
