pub mod messenger;

use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use protos::rpc::v1;
use protos::rpc::v1::zk_messenger_service_server::ZkMessengerServiceServer;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use std::path::PathBuf;
use std::{net::SocketAddr, str::FromStr};
use tonic::service::Routes;
use tracing::{error, info};

use crate::messenger::MessengerRpcServer;

pub struct ServerConfig {
    /// gRPC address at which the server will listen for incoming connections.
    pub grpc_address: String,
    /// TLS configuration
    pub tls_config: Option<TlsConfig>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Path to PEM encoded X509 Certificate file
    pub cert_path: PathBuf,

    /// Path to a private key file for a TLS certificate
    pub key_path: PathBuf,
}

pub async fn run_server(
    ServerConfig {
        grpc_address,
        tls_config,
    }: ServerConfig,
    cancellation: CancellationToken,
) -> eyre::Result<()>
where
{
    let http_routes = Router::new();

    let spark_server = MessengerRpcServer::new();

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(v1::FILE_DESCRIPTOR_SET)
        .build_v1alpha()?;

    let mut grpc_builder = Routes::builder();
    grpc_builder.add_service(ZkMessengerServiceServer::new(spark_server));
    grpc_builder.add_service(reflection_service);
    let grpc_routes = grpc_builder
        .routes()
        .into_axum_router()
        .layer(tonic_web::GrpcWebLayer::new());

    let app = http_routes.merge(grpc_routes);

    if let Some(TlsConfig {
        cert_path,
        key_path,
    }) = tls_config
    {
        let config = RustlsConfig::from_pem_file(cert_path, key_path)
            .await
            .inspect_err(|err| error!("Failed to create TLS config: {}", err))?;

        info!("Starting gRPC server (w/ TLS) on {}", grpc_address);
        axum_server::bind_rustls(SocketAddr::from_str(&grpc_address)?, config)
            .serve(app.into_make_service())
            .await?;
    } else {
        info!("Starting gRPC server (w/o TLS) on {}", grpc_address);
        axum_server::bind(SocketAddr::from_str(&grpc_address)?)
            .serve(app.into_make_service())
            .await?;
    }

    info!("gRPC server started");

    // Await until stop message received
    cancellation.cancelled().await;

    Ok(())
}
