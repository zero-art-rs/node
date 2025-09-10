use mongodb::Database;
use std::sync::OnceLock;

mod impls;
mod traits;

pub use impls::*;
pub use mongodb::error::Error;
pub use traits::*;

pub use types::errors::StorageError;

#[derive(Debug)]
pub struct MongoConfig {
    pub uri: String,
    pub database_name: String,
}

pub static DATABASE: OnceLock<Database> = OnceLock::new();
