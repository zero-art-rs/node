use std::sync::Arc;

use axum::{Router, middleware, routing::get};
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
#[openapi(
    info(title = env!("CARGO_PKG_NAME"),),
    components(schemas(
        domains::auth::transport::http::AuthRequest,
        domains::auth::transport::http::AuthResponse,
    ))
)]
struct PublicApiDoc;

pub fn build_router(container: Arc<Container>) -> Router<Arc<Container>> {
    let public_routes = OpenApiRouter::new()
        .routes(routes![get_health_handler])
        .routes(routes!(domains::auth::transport::http::authenticate))
        // TODO: add auth middleware to SSE
        .routes(routes!(
            domains::messenger::transport::sse::subscribe_for_messages
        ));

    let protected_routes = OpenApiRouter::new()
        .routes(routes!(domains::messenger::transport::http::send_message))
        .routes(routes!(domains::messenger::transport::http::list_messages))
        .routes(routes!(
            domains::messenger::transport::http::delete_messages
        ))
        .routes(routes!(domains::messenger::transport::http::mark_as_read))
        .routes(routes!(domains::messenger::transport::http::delete_cursors))
        .routes(routes!(domains::messenger::transport::http::list_cursors))
        .routes(routes!(domains::messenger::transport::http::init_chat))
        .routes(routes!(domains::messenger::transport::http::get_art))
        .routes(routes!(domains::messenger::transport::http::list_changes))
        .routes(routes!(domains::messenger::transport::http::update_art))
        .layer(middleware::from_fn_with_state(
            container,
            domains::auth::transport::http::jwt_middleware,
        ));

    let shared_routes = public_routes.merge(protected_routes);

    let (router, public_api) = OpenApiRouter::with_openapi(PublicApiDoc::openapi())
        .merge(shared_routes.clone())
        .split_for_parts();
    let public_swagger = SwaggerUi::new("/swagger").url("/spec.json", public_api.clone());
    router.merge(public_swagger)
}
