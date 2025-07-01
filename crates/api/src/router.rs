use std::sync::Arc;

use axum::{Router, middleware, routing::get};
use utoipa::openapi::security::{ApiKey, ApiKeyValue, Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};
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

pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = &mut openapi.components {
            let http = Http::new(HttpAuthScheme::Bearer);

            components
                .security_schemes
                .insert("bearer_auth".to_string(), SecurityScheme::Http(http));
        }
    }
}

#[derive(utoipa::OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    info(title = env!("CARGO_PKG_NAME"),),
    components(schemas(
        domains::centrifugo::transport::http::AuthRequest,
        domains::centrifugo::transport::http::AuthResponse,
    ))
)]
struct PublicApiDoc;

pub fn build_router() -> Router<Arc<Container>> {
    let routes = OpenApiRouter::new()
        .routes(routes![get_health_handler])
        .routes(routes!(domains::centrifugo::transport::http::authenticate))
        .routes(routes!(domains::messenger::transport::http::send_message))
        .routes(routes!(domains::messenger::transport::http::list_messages))
        .routes(routes!(
            domains::messenger::transport::http::delete_messages
        ))
        .routes(routes!(domains::messenger::transport::http::mark_as_read))
        .routes(routes!(domains::messenger::transport::http::delete_cursors))
        .routes(routes!(domains::messenger::transport::http::list_cursors))
        .routes(routes!(domains::art::transport::http::init_chat_phase1))
        .routes(routes!(domains::art::transport::http::init_chat_phase2))
        .routes(routes!(domains::art::transport::http::init_chat_phase3))
        .routes(routes!(domains::art::transport::http::add_member_phase1))
        .routes(routes!(domains::art::transport::http::add_member_phase2))
        .routes(routes!(domains::art::transport::http::add_member_phase3))
        .routes(routes!(domains::art::transport::http::remove_member))
        // .routes(routes!(domains::art::transport::http::leave_chat))
        .routes(routes!(domains::art::transport::http::update_key))
        .routes(routes!(domains::art::transport::http::get_changes))
        .routes(routes!(domains::art::transport::http::delete_chat))
        .routes(routes!(
            domains::invitations::transport::http::add_invitations
        ))
        .routes(routes!(
            domains::invitations::transport::http::add_member_invitation
        ))
        .routes(routes!(
            domains::invitations::transport::http::get_invitation
        ))
        .routes(routes!(
            domains::invitations::transport::http::delete_invitation
        ));

    let (router, public_api) = OpenApiRouter::with_openapi(PublicApiDoc::openapi())
        .merge(routes)
        .split_for_parts();
    let public_swagger = SwaggerUi::new("/swagger").url("/spec.json", public_api.clone());
    router.merge(public_swagger)
}
