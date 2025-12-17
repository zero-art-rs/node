use crate::container::Container;
use crate::verification_middleware;
use crate::{art_transport, centrifugo_transport, messenger_transport};
use axum::{Router, middleware};
use std::sync::Arc;
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

pub fn build_router(container: Arc<Container>) -> Router<Arc<Container>> {
    let health_handler_route = OpenApiRouter::new().routes(routes![get_health_handler]);

    //Messages:
    let list_messages_route = OpenApiRouter::new()
        .routes(routes![messenger_transport::list_messages])
        .route_layer(middleware::from_fn_with_state(
            container.clone(),
            verification_middleware::verify_list_messages,
        ));

    let count_messages_route = OpenApiRouter::new()
        .routes(routes![messenger_transport::count_messages])
        .route_layer(middleware::from_fn_with_state(
            container.clone(),
            verification_middleware::verify_list_messages,
        ));

    // Centrifugo:
    let authenticate_route = OpenApiRouter::new()
        .routes(routes![centrifugo_transport::authenticate])
        .route_layer(middleware::from_fn_with_state(
            container.clone(),
            verification_middleware::verify_authentication,
        ));

    // Group management:
    let send_frame_route = OpenApiRouter::new().routes(routes![messenger_transport::send_frame]);

    let get_art_route = OpenApiRouter::new()
        .routes(routes![art_transport::get_art])
        .route_layer(middleware::from_fn_with_state(
            container.clone(),
            verification_middleware::get_art,
        ));

    let get_challenge_route = OpenApiRouter::new().routes(routes![art_transport::get_challenge]);

    // Combine routers in one OpenApiRouter
    let (router, public_api) = OpenApiRouter::with_openapi(PublicApiDoc::openapi())
        .merge(health_handler_route)
        // Messages
        .merge(list_messages_route)
        .merge(count_messages_route)
        // Centrifugo
        .merge(authenticate_route)
        // Group operations
        .merge(send_frame_route)
        .merge(get_art_route)
        .merge(get_challenge_route)
        .split_for_parts();

    // Add public swagger for node
    let public_swagger = SwaggerUi::new("/swagger").url("/spec.json", public_api.clone());
    router.merge(public_swagger)
}
