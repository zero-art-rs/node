use crate::router::build_router;
use axum::{
    body::{Body, Bytes},
    extract::{MatchedPath, Request, State},
    http::{HeaderMap, StatusCode},
    middleware,
    middleware::Next,
    response::Response,
};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use tower_http::{classify::ServerErrorsFailureClass, cors::CorsLayer, trace::TraceLayer};
use tracing::{Span, info, info_span};

mod container;
pub(crate) mod domains;
mod errors;
mod router;

#[cfg(all(test, feature = "integration-tests"))]
mod tests;
mod utils;
pub mod verification;

pub use container::Container;
pub use domains::art::service::ARTService;
pub use domains::centrifugo::service::CentrifugoService;
pub use domains::messenger::service::MessengerService;
pub(crate) use utils::as_base64;

pub async fn run_server(
    address: String,
    container: Arc<Container>,
    cancellation: CancellationToken,
) -> eyre::Result<()> {
    info!("Starting API server on {}", address);
    let listener = tokio::net::TcpListener::bind(address).await?;

    axum::serve(
        listener,
        build_router()
            .layer(CorsLayer::permissive())
            .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request<_>| {
                    let matched_path = request
                        .extensions()
                        .get::<MatchedPath>()
                        .map(MatchedPath::as_str);

                    info_span!(
                        "http_request",
                        method = ?request.method(),
                        matched_path,
                    )
                })
                .on_request(|request: &Request<_>, span: &Span| {
                    tracing::info!(parent: span, "Incoming request: {} {}", request.method(), request.uri());
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
            .layer(middleware::from_fn_with_state(container.clone(), verification::verification_middleware))
            .with_state(container),
    ).with_graceful_shutdown(cancellation.cancelled_owned()).await?;

    Ok(())
}
