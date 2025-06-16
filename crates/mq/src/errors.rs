use deadpool_lapin::{
    PoolError,
    CreatePoolError,
    lapin::Error as DeadpoolLapinError,
};
use lapin::{
    Error as LapinError,
};
use serde_json::Error as SerdeJsonError;

/// Message Query errors
#[derive(Debug, thiserror::Error)]
pub enum MqError {
    #[error("lapin error: {0}")]
    Lapin(#[from] LapinError),
    
    #[error("deadpool-lapin error: {0}")]
    DeadpoolLapinError(#[from] DeadpoolLapinError),
    
    #[error("Pool error: {0}")]
    Pool(#[from] PoolError),
    
    #[error("Create connection pool error: {0}")]
    CreatePool(#[from] CreatePoolError),
    
    #[error("Serde JSON error: {0}")]
    SerdeJson(#[from] SerdeJsonError),
}
