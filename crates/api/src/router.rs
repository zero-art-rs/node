use std::sync::Arc;

use axum::Router;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

use crate::{container::Container, domains};

#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Healthy", body = String),
    )
)]
async fn get_health_handler() -> &'static str {
    "healthy"
}

#[derive(utoipa::OpenApi)]
#[openapi(info(title = env!("CARGO_PKG_NAME"),))]
struct PublicApiDoc;

pub fn build_router() -> Router<Arc<Container>> {
    let shared_routes = OpenApiRouter::new()
        .routes(routes![get_health_handler])
        // auth service
        // message service
        .routes(routes!(domains::messenger::transport::http::send_message))
        .routes(routes!(domains::messenger::transport::http::list_messages))
        .routes(routes!(
            domains::messenger::transport::http::delete_messages
        ))
        .routes(routes!(domains::messenger::transport::http::mark_as_read))
        .routes(routes!(domains::messenger::transport::http::delete_cursors))
        .routes(routes!(domains::messenger::transport::http::list_cursors));
    let (router, public_api) = OpenApiRouter::with_openapi(PublicApiDoc::openapi())
        .merge(shared_routes.clone())
        .split_for_parts();
    let public_swagger = SwaggerUi::new("/swagger").url("/spec.json", public_api.clone());
    router.merge(public_swagger)
}
