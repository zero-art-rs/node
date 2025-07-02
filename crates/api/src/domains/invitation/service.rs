use ark_ec::AffineRepr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, SerializationError};
use ark_std::UniformRand;
use ark_std::rand::SeedableRng;
use ark_std::rand::prelude::StdRng;
use art::{ART, BranchChanges, BranchChangesType};
use bson::{Binary, Bson, doc, spec::BinarySubtype};
use mongodb::bson::Uuid;
use mongodb::bson::{self, to_bson};
use mongodb::bson::{DateTime, Document};
use serde::{Serialize, Serializer};
use std::sync::Arc;
use storage::{
    ARTChangesStorage, ARTStorage, DataStorage, MongoARTChangesStorage, MongoARTStorage,
    MongoInvitationStorage,
};
use tracing::{debug, error, info};
use types::{ARTChangesRecord, ARTRecord, CursorRecord, InvitationRecord, Message};
use zk::curve::cortado::{CortadoAffine as ARTGroup, Fr as ScalarField};

#[derive(Debug, thiserror::Error)]
pub enum InvitationServiceError {
    #[error("Storage error: {0}")]
    StorageError(storage::Error),
    #[error("Conversion error: {0}")]
    ConversionError(bson::oid::Error),
    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<storage::Error> for InvitationServiceError {
    fn from(error: storage::Error) -> Self {
        InvitationServiceError::StorageError(error)
    }
}

pub struct InvitationService {}

impl InvitationService {
    pub fn new() -> Self {
        Self {}
    }
}

impl InvitationService {
    pub async fn add_invitation(
        &self,
        chat_id: &Uuid,
        invitation: InvitationRecord<ARTGroup>,
    ) -> Result<(), InvitationServiceError> {
        let invitation_storage = self.create_invitations_storage(&chat_id).await?;

        // invitation_storage.find_one(
        //     doc! {"receiver": bson_receiver}
        // ).await?;

        invitation_storage.store(invitation).await?;

        Ok(())
    }

    pub async fn store_invitations(
        &self,
        chat_id: &Uuid,
        invitations: &Vec<InvitationRecord<ARTGroup>>,
    ) -> Result<(), InvitationServiceError> {
        let storage = self
            .create_invitations_storage(chat_id)
            .await
            .map_err(|e| InvitationServiceError::StorageError(e))?;

        let mut invitation_records = Vec::new();
        for invite in invitations {
            invitation_records.push(invite.clone());
        }
        storage.store_many(invitation_records).await?;

        Ok(())
    }

    pub async fn get_invitation(
        &self,
        chat_id: &Uuid,
        receiver: &Vec<u8>,
    ) -> Result<InvitationRecord<ARTGroup>, InvitationServiceError> {
        let storage = self
            .create_invitations_storage(chat_id)
            .await
            .map_err(|e| InvitationServiceError::StorageError(e))?;

        let filter = doc! {"receiver_public_key": Binary {
            subtype: BinarySubtype::Generic,
            bytes: receiver.clone(),
        }};

        let invitation = storage
            .find_one(filter)
            .await
            .map_err(|e| InvitationServiceError::StorageError(e))?;

        match invitation {
            Some(invite) => Ok(invite),
            None => Err(InvitationServiceError::InternalError(
                "failed to get invitation".to_string(),
            )),
        }
    }

    pub async fn delete_invitation(
        &self,
        chat_id: &Uuid,
        receiver: &Vec<u8>,
    ) -> Result<(), InvitationServiceError> {
        let storage = self
            .create_invitations_storage(chat_id)
            .await
            .map_err(|e| InvitationServiceError::StorageError(e))?;

        let filter = doc! {"receiver_public_key": Binary {
            subtype: BinarySubtype::Generic,
            bytes: receiver.clone(),
        }};

        storage
            .delete(filter)
            .await
            .map_err(|e| InvitationServiceError::StorageError(e))?;

        Ok(())
    }

    async fn create_invitations_storage(
        &self,
        chat_id: &Uuid,
    ) -> Result<Arc<MongoInvitationStorage>, mongodb::error::Error> {
        Ok(Arc::new(MongoInvitationStorage::new(chat_id).await?))
    }
}
