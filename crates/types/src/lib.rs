pub mod callback_wrappers;
pub mod errors;
mod message;
mod records;
mod schemas;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
pub use message::{Message, Subscription};
pub use records::{ARTChangesOutboxRecord, ARTChangesRecord, ARTRecord, Record};
pub use schemas::{art_schemas, centrifugo_schemas, messenger_schemas};

#[derive(Clone, Debug)]
pub struct RouteId(pub &'static str);

/// The middleware that adds the identifier to extensions
pub async fn add_route_id(id: &'static str, mut req: Request, next: Next) -> Response {
    req.extensions_mut().insert(RouteId(id));
    next.run(req).await
}
