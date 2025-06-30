use crate::DataStorage;

#[async_trait::async_trait]
pub trait InvitationStorage: Send + Sync + DataStorage {}
