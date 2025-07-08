use bson::{Binary, doc, spec::BinarySubtype};
use mongodb::bson::{self};
use storage::{DataStorage, MongoInvitationStorage, StorageError};
use tracing::error;
use types::InvitationRecord;
use uuid::Uuid;
use cortado::CortadoAffine as ARTGroup;

#[derive(Debug, thiserror::Error)]
pub enum InvitationServiceError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Record not found")]
    NotFound,
}

pub struct InvitationService {}

impl InvitationService {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for InvitationService {
    fn default() -> Self {
        Self::new()
    }
}

impl InvitationService {
    pub async fn add_invitation(
        &self,
        chat_id: &Uuid,
        invitation: InvitationRecord<ARTGroup>,
    ) -> Result<(), InvitationServiceError> {
        let invitation_storage = MongoInvitationStorage::new(chat_id).await?;

        invitation_storage.insert_one(invitation).await?;

        Ok(())
    }

    pub async fn store_invitations(
        &self,
        chat_id: &Uuid,
        invitations: &Vec<InvitationRecord<ARTGroup>>,
    ) -> Result<(), InvitationServiceError> {
        let storage = MongoInvitationStorage::new(chat_id).await?;

        let mut invitation_records = Vec::new();
        for invite in invitations {
            invitation_records.push(invite.clone());
        }
        storage.insert_many(invitation_records).await?;

        Ok(())
    }

    pub async fn get_invitation(
        &self,
        chat_id: &Uuid,
        receiver: &[u8],
    ) -> Result<InvitationRecord<ARTGroup>, InvitationServiceError> {
        let storage = MongoInvitationStorage::new(chat_id).await?;

        let filter = doc! {"receiver_public_key": Binary {
            subtype: BinarySubtype::Generic,
            bytes: receiver.to_owned(),
        }};

        let invitation = storage.find_one(filter).await?;

        match invitation {
            Some(invite) => Ok(invite),
            None => Err(InvitationServiceError::NotFound),
        }
    }

    pub async fn delete_invitation(
        &self,
        chat_id: &Uuid,
        receiver: &[u8],
    ) -> Result<(), InvitationServiceError> {
        let storage = MongoInvitationStorage::new(chat_id).await?;

        let filter = doc! {"receiver_public_key": Binary {
            subtype: BinarySubtype::Generic,
            bytes: receiver.to_owned(),
        }};

        storage.delete(filter).await?;

        Ok(())
    }
}
