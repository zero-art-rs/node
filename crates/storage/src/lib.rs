use mongodb::{Client, Database};
use std::sync::OnceLock;

mod errors;
mod impls;
mod traits;

pub use impls::MongoARTChangesStorage;
pub use impls::MongoARTStorage;
pub use impls::MongoCursorStorage;
pub use impls::MongoInvitationStorage;
pub use impls::MongoMessageStorage;
pub use mongodb::error::Error;
pub use traits::ARTChangesStorage;
pub use traits::ARTStorage;
pub use traits::CursorStorage;
pub use traits::DataStorage;
pub use traits::InvitationStorage;
pub use traits::MessageStorage;

pub use errors::StorageError;

#[derive(Debug)]
pub struct MongoConfig {
    pub uri: String,
    pub database_name: String,
}

pub static DATABASE: OnceLock<Database> = OnceLock::new();
pub static CLIENT: OnceLock<Client> = OnceLock::new();
