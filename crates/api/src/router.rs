use axum::{Router, middleware};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

use crate::{container::Container, domains, verification};

use crate::domains::art::transport::http as art_transport;
use crate::domains::centrifugo::transport::http as centrifugo_transport;
use crate::domains::messenger::transport::http as messenger_transport;

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
        domains::centrifugo::transport::http::AuthRequest,
        domains::centrifugo::transport::http::AuthResponse,
    )),
)]
struct PublicApiDoc;

pub fn build_router(container: Arc<Container>) -> Router<Arc<Container>> {
    let routes = OpenApiRouter::new()
        .routes(routes![get_health_handler])
        .routes(routes!(centrifugo_transport::authenticate))
        .routes(routes!(art_transport::init_chat))
        .routes(routes!(art_transport::get_challenge));

    let protected_routes = OpenApiRouter::new()
        .routes(routes!(messenger_transport::send_message))
        .routes(routes!(messenger_transport::list_messages))
        .routes(routes!(messenger_transport::delete_messages))
        .routes(routes!(art_transport::get_art))
        .routes(routes!(art_transport::get_initial_art))
        .routes(routes!(art_transport::add_member))
        .routes(routes!(art_transport::remove_member))
        .routes(routes!(art_transport::update_key))
        .routes(routes!(art_transport::get_changes))
        .routes(routes!(art_transport::delete_chat))
        .layer(middleware::from_fn_with_state(
            container,
            verification::verification_middleware,
        ));

    let (router, public_api) = OpenApiRouter::with_openapi(PublicApiDoc::openapi())
        .merge(routes)
        .merge(protected_routes)
        .split_for_parts();
    let public_swagger = SwaggerUi::new("/swagger").url("/spec.json", public_api.clone());
    router.merge(public_swagger)
}
