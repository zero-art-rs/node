use std::sync::Arc;
use std::time::Duration;

use crate::config::NodeConfig;
use api::{AuthService, Container, MessengerService};
use eyre::Ok;
use mongodb::{
    Client, Collection, IndexModel, bson,
    bson::spec::BinarySubtype,
    bson::{Binary, DateTime, Document, doc},
    options::{ClientOptions, IndexOptions},
};
use storage::{DATABASE, MongoConfig, MongoMessageStorage};
use tokio::select;
use tokio::time::sleep;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::info;

use mongodb::bson::Uuid;

/// The limit of time to wait for the node to shutdown.
const DEFAULT_SHUTDOWN_TIMEOUT_SECS: u64 = 30;

/// Node encapsulate node service's start
pub struct Node {
    config: NodeConfig,
    cancelation: CancellationToken,

    pub(crate) task_tracker: TaskTracker,
}

impl Node {
    pub async fn new(config: NodeConfig) -> eyre::Result<Self> {
        Ok(Self {
            config,
            cancelation: CancellationToken::new(),
            task_tracker: TaskTracker::new(),
        })
    }

    /// Wait for the signal from any node's service about the cancellation.
    pub async fn cancelled(&self) {
        self.cancelation.cancelled().await
    }

    pub async fn run(&self) -> eyre::Result<()> {
        let uri = self.config.storage.database_url.clone();
        let database_name = self.config.storage.database_name.clone();

        let client_options = ClientOptions::parse(uri).await?;
        let client = Client::with_options(client_options)?;

        DATABASE
            .set(
                client
                    .default_database()
                    .unwrap_or(client.database(&database_name)),
            )
            .unwrap();
        DATABASE
            .get()
            .unwrap()
            .run_command(doc! { "ping": 1 })
            .await?;
        info!("Connected to database {}", database_name);

        // create default chat api
        self.spawn_api().await?;

        self.task_tracker.close();

        Ok(())
    }

    async fn spawn_api(&self) -> eyre::Result<()> {
        let address = self.config.api.address.to_string();
        let messenger_service = MessengerService::new();
        let auth_service = AuthService::new(self.config.jwt.secret.clone(), self.config.jwt.ttl);

        let container = Arc::new(Container {
            messenger_service: Arc::new(messenger_service),
            auth_service: Arc::new(auth_service),
        });

        self.task_tracker.spawn(api::run_server(address, container));

        Ok(())
    }

    pub async fn shutdown(&self) {
        info!("Shutting down node, finishing received requests...");

        self.cancelation.cancel();

        select! {
            // Wait until all tasks are finished
            _ = self.task_tracker.wait() => {},
            // Or wait for and exit by timeout
            _ = sleep(Duration::from_secs(DEFAULT_SHUTDOWN_TIMEOUT_SECS)) => {
                info!("Shutdown timeout reached, exiting...");
            },
        }
    }
}
