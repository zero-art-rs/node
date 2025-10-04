use std::sync::Arc;
use tokio::select;
use tokio::signal::unix;
use tokio::signal::unix::SignalKind;

use crate::{
    cli::{arguments, logging, node::Node},
    config::NodeConfig,
};

pub async fn run(args: arguments::Run) -> eyre::Result<()> {
    let config = NodeConfig::from_path(args.config)?;

    logging::init(config.logger.level)?;

    let node = Arc::new(Node::new(config).await?);
    let node_clone = node.clone();

    tokio::spawn(async move {
        let res = node_clone.run().await;
        node_clone.task_tracker.close();

        if let Err(err) = res {
            tracing::error!("Node run failed: {:?}", err);
            node_clone.shutdown().await;
        }
    });

    let mut sigterm =
        unix::signal(SignalKind::terminate()).expect("Failed to create SIGTERM signal handler");
    let mut sigint =
        unix::signal(SignalKind::interrupt()).expect("Failed to create SIGINT signal handler");

    select! {
        _ = node.cancelled() => {
            tracing::error!("Node run failed");
        }
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM signal");
        }
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT signal");
        }
    }

    node.shutdown().await;

    Ok(())
}
