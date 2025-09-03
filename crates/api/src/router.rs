use crate::container::Container;
use crate::domains::art::transport::http as art_transport;
use crate::domains::centrifugo::transport::http as centrifugo_transport;
use crate::domains::messenger::transport::http::*;
use crate::verification_middleware::verification_middleware;
use axum::{Router, middleware};
use std::sync::Arc;
use types::add_route_id;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use utoipa_swagger_ui::SwaggerUi;

/// Check health
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
        types::centrifugo_schemas::AuthRequest,
        types::centrifugo_schemas::AuthResponse,
    )),
)]
struct PublicApiDoc;

trait VerificationLayer {
    fn with_verification(self, container: Arc<Container>) -> Self;
}

impl<S> VerificationLayer for OpenApiRouter<S>
where
    S: Send + Sync + Clone + 'static,
{
    fn with_verification(self, container: Arc<Container>) -> Self {
        self.route_layer(middleware::from_fn_with_state(
            container,
            verification_middleware,
        ))
    }
}

trait RouteIdLayer {
    fn with_route_id(self, id: &'static str) -> Self;
}

impl<S> RouteIdLayer for OpenApiRouter<S>
where
    S: Send + Sync + Clone + 'static,
{
    fn with_route_id(self, id: &'static str) -> Self {
        self.route_layer(middleware::from_fn(move |req, next| {
            add_route_id(id, req, next)
        }))
    }
}

pub fn build_router(container: Arc<Container>) -> Router<Arc<Container>> {
    //Messages:
    let health_handler_route = OpenApiRouter::new().routes(routes![get_health_handler]);

    let list_messages_route = OpenApiRouter::new()
        .routes(routes![list_messages])
        .with_verification(container.clone())
        .with_route_id("list_messages");

    let count_messages_route = OpenApiRouter::new()
        .routes(routes![count_messages])
        .with_verification(container.clone())
        .with_route_id("count_messages");

    let send_message_route = OpenApiRouter::new()
        .routes(routes![send_message])
        .with_verification(container.clone())
        .with_route_id("send_message");

    // Centrifugo:
    let authenticate_route = OpenApiRouter::new()
        .routes(routes![centrifugo_transport::authenticate])
        .with_verification(container.clone())
        .with_route_id("authenticate");

    // Group management:
    let init_chat_route = OpenApiRouter::new().routes(routes![art_transport::init_chat]);

    let get_art_route = OpenApiRouter::new()
        .routes(routes![art_transport::get_art])
        .with_verification(container.clone())
        .with_route_id("get_art");

    let get_changes_route = OpenApiRouter::new()
        .routes(routes![art_transport::get_changes])
        .with_verification(container.clone())
        .with_route_id("get_changes");

    let update_art_route = OpenApiRouter::new()
        .routes(routes![art_transport::update_art])
        .with_verification(container.clone())
        .with_route_id("update_art");

    let get_challenge_route = OpenApiRouter::new().routes(routes![art_transport::get_challenge]);

    let delete_chat_route = OpenApiRouter::new()
        .routes(routes![art_transport::delete_chat])
        .with_verification(container.clone())
        .with_route_id("delete_chat");

    let (router, public_api) = OpenApiRouter::with_openapi(PublicApiDoc::openapi())
        .merge(health_handler_route)
        .merge(list_messages_route)
        .merge(count_messages_route)
        .merge(send_message_route)
        .merge(authenticate_route)
        .merge(init_chat_route)
        .merge(get_art_route)
        .merge(get_changes_route)
        .merge(update_art_route)
        .merge(get_challenge_route)
        .merge(delete_chat_route)
        .split_for_parts();

    let public_swagger = SwaggerUi::new("/swagger").url("/spec.json", public_api.clone());
    router.merge(public_swagger)
}
