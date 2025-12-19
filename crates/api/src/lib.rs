use crate::router::build_router;
use axum::{extract::Request, response::Response};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tower_http::{classify::ServerErrorsFailureClass, cors::CorsLayer, trace::TraceLayer};
use tracing::{Span, info, info_span};

mod container;
pub(crate) mod domains;
mod router;
mod verification_middleware;

pub use container::Container;
pub use domains::art::service::ARTService;
pub use domains::centrifugo::service::CentrifugoService;
pub use domains::messenger::service::MessengerService;

pub use domains::art::http as art_transport;
pub use domains::centrifugo::http as centrifugo_transport;
pub use domains::messenger::http as messenger_transport;

pub async fn run_server(
    address: String,
    container: Arc<Container>,
    cancellation: CancellationToken,
) -> eyre::Result<()> {
    #[cfg(not(feature = "verification"))]
    {
        warn!("Verification middleware is disabled.");
    }
    info!("Starting API server on {}", address);
    let listener = tokio::net::TcpListener::bind(address).await?;

    axum::serve(
        listener,
        build_router(container.clone())
            .layer(CorsLayer::permissive())
            .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    info_span!(
                        "http_request",
                        method = ?request.method(),
                        request_id = ?uuid::Uuid::new_v4(),
                    )
                })
                .on_request(|request: &Request<_>, span: &Span| {
                    tracing::info!(parent: span, "Incoming request: {} {}", request.method(), request.uri().path());
                })
                .on_response(|response: &Response, latency: Duration, span: &Span| {
                    tracing::info!(parent: span, status = ?response.status(), latency = ?latency, "Response sent");
                })
                .on_failure(
                    |error: ServerErrorsFailureClass, latency: Duration, span: &Span| {
                        tracing::error!(parent: span, error = ?error, latency = ?latency, "Request failed");
                    },
                ),
            )
            .with_state(container),
    ).with_graceful_shutdown(cancellation.cancelled_owned()).await?;

    Ok(())
}
