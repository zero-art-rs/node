use std::sync::Arc;

use axum::{Router, middleware};
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
        .routes(routes!(domains::auth::transport::http::authenticate));

    let protected_routes = OpenApiRouter::new()
        .routes(routes!(domains::messenger::transport::http::send_message))
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
