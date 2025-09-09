pub mod callback_wrappers;
pub mod errors;
mod records;
mod schemas;
pub mod utils;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
pub use records::{ARTRecord, MessageRecord, Subscription};
pub use schemas::{art_schemas, centrifugo_schemas, messenger_schemas};
pub mod protos {
    include!(concat!(env!("OUT_DIR"), "/zero_art_proto.rs"));
}

#[derive(Clone, Debug)]
pub struct RouteId(pub &'static str);

/// The middleware that adds the identifier to extensions
pub async fn add_route_id(id: &'static str, mut req: Request, next: Next) -> Response {
    req.extensions_mut().insert(RouteId(id));
    next.run(req).await
}

pub(crate) const DEFAULT_LIMIT: i64 = 10;
pub(crate) const DEFAULT_SKIP: i64 = 0;

pub(crate) const fn default_limit() -> i64 {
    DEFAULT_LIMIT
}
pub(crate) const fn default_skip() -> i64 {
    DEFAULT_SKIP
}
