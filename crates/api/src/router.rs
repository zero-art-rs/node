use std::sync::Arc;

use axum::Router;
use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_swagger_ui::SwaggerUi;

use crate::{container::Container, domains};

use crate::domains::art::transport::http as art_transport;
use crate::domains::centrifugo::transport::http as centrifugo_transport;
use crate::domains::invitation::transport::http as invitation_transport;
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
    info(title = env!("CARGO_PKG_NAME"),),
    components(schemas(
        domains::centrifugo::transport::http::AuthRequest,
        domains::centrifugo::transport::http::AuthResponse,
    )),
    modifiers(&SecurityAddon),
)]
struct PublicApiDoc;

pub fn build_router() -> Router<Arc<Container>> {
    let routes = OpenApiRouter::new()
        .routes(routes![get_health_handler])
        .routes(routes!(centrifugo_transport::authenticate))
        .routes(routes!(messenger_transport::send_message))
        .routes(routes!(messenger_transport::list_messages))
        .routes(routes!(messenger_transport::delete_messages))
        .routes(routes!(messenger_transport::mark_as_read))
        .routes(routes!(messenger_transport::delete_cursors))
        .routes(routes!(messenger_transport::list_cursors))
        .routes(routes!(art_transport::init_chat))
        .routes(routes!(art_transport::get_art))
        .routes(routes!(art_transport::add_member))
        .routes(routes!(art_transport::remove_member))
        // .routes(routes!(art_transport::leave_chat))
        .routes(routes!(art_transport::update_key))
        .routes(routes!(art_transport::get_changes))
        .routes(routes!(art_transport::delete_chat))
        .routes(routes!(art_transport::list_chats))
        .routes(routes!(invitation_transport::add_invitations))
        .routes(routes!(invitation_transport::add_member_invitation))
        .routes(routes!(invitation_transport::get_invitation))
        .routes(routes!(invitation_transport::delete_invitation));

    let (router, public_api) = OpenApiRouter::with_openapi(PublicApiDoc::openapi())
        .merge(routes)
        .split_for_parts();
    let public_swagger = SwaggerUi::new("/swagger").url("/spec.json", public_api.clone());
    router.merge(public_swagger)
}
