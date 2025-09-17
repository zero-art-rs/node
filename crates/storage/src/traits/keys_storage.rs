use crate::DataStorage;

/// Storage for public keys.
///
/// # Associated Types
///
/// - [`Error`](FrameStorage::Error):
///   The error type returned by storage operations.
///
/// - [`Session`](FrameStorage::Session):
///   A database session or transaction handle, used to group operations
///   into a consistent context.
#[async_trait::async_trait]
pub trait KeyStorage: DataStorage<Self::Data, Self::Error, Self::Session> + Send + Sync + Sized {
    type Data;
    type Session;
    type Error;

    async fn new() -> Result<Self, Self::Error>;
}