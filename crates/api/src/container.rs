use crate::domains::{
    art::service::ARTService, centrifugo::service::CentrifugoService,
    messenger::service::MessengerService,
};
use ark_serialize::CanonicalDeserialize;
use art::types::BranchChangesType;
use axum::body::Bytes;
use axum::http::StatusCode;
use bytes::BytesMut;
use cortado::CortadoAffine;
use mongodb::bson::doc;
use proof_verifier::ProofVerifierSender;
use prost::Message;
use std::collections::HashSet;
use std::sync::Arc;
use storage::{ARTStorage, MongoARTStorage};
use tokio::sync::RwLock;
use tracing::debug;
use types::errors::{ARTServiceError, ServiceError, StorageError};
use types::protos::group_operation::Operation;
use types::protos::{Frame, FrameTbs};
use types::utils::decode_branch_changes;
use uuid::Uuid;

pub struct Container {
    pub messenger_service: Arc<MessengerService>,
    pub centrifugo_service: Arc<CentrifugoService>,
    pub art_service: Arc<ARTService>,

    pub proof_verifier_sender: ProofVerifierSender,

    pub art_is_updating: Arc<RwLock<HashSet<Uuid>>>,
    pub challenges: Arc<RwLock<HashSet<Vec<u8>>>>,
}

impl Container {
    pub async fn art_is_updating(&self, chat_id: Uuid) -> bool {
        self.art_is_updating.read().await.contains(&chat_id)
    }

    pub async fn start_updating(&self, chat_id: Uuid) -> Result<(), ServiceError> {
        if self.art_is_updating(chat_id).await {
            debug!("Failed to update art. It is currently changing");
            Err(ServiceError::from(ARTServiceError::ArtIsChanging))
        } else {
            self.art_is_updating.write().await.insert(chat_id);
            Ok(())
        }
    }

    pub async fn stop_updating(&self, chat_id: Uuid) {
        self.art_is_updating.write().await.remove(&chat_id);
    }

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
            None => StatusCode::OK,
        };

        self.messenger_service
            .send_message(body.to_vec(), &id, tbs_frame.epoch as i64, false)
            .await?;

        Ok(response)
    }

    pub async fn update_art(
        &self,
        id: Uuid,
        branch_changes_bytes: Vec<u8>,
        update_epoch: u64,
    ) -> Result<StatusCode, ServiceError> {
        // self.start_updating(chat_id).await?;

        let branch_changes =
            decode_branch_changes(&branch_changes_bytes).map_err(ARTServiceError::from)?;

        let current_epoch = self.art_service.get_current_epoch(id).await?;

        match update_epoch {
            e if e == current_epoch => {
                // resolve merge conflict
                // let applied_changes = self.messenger_service.get_changes(doc! {"epoch": e});
            }
            e if e == current_epoch + 1 => {
                // update art
            }
            _ => {}
        }

        if let Err(e) = self.art_service.update_art(id, &branch_changes).await {
            // self.stop_updating(chat_id).await;
            return Err(e.into());
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
