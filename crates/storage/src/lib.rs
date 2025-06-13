use async_trait::async_trait;
use mongodb::{
    bson::{doc, Document},
    options::{ClientOptions, IndexOptions},
    Client, Collection, IndexModel,
};
use serde::Serialize;

mod impls;
mod traits;

pub use impls::MongoMessageStorage;
pub use mongodb::error::Error;
pub use traits::MessageStorage;

#[derive(Debug)]
pub struct MongoConfig {
    pub uri: String,
    pub database_name: String,
}
