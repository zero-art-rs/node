use api::{art_transport, centrifugo_transport, messenger_transport};
use axum::Router;
use axum::routing::get;
use axum_test::TestServer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

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

#[tokio::test]
async fn it_should_be_healthy() {
    // Build an application with a route.
    let app = Router::new().route(&"/health", get(get_health_handler));

    // Run the application for testing.
    let server = TestServer::new(app).unwrap();

    // Get the request.
    let response = server.get("/health").await;

    // Assertions.
    response.assert_status_ok();
    response.assert_text("healthy");
}

// #[tokio::test]
// pub fn test_send_message_endpoint() {
//     let request = axum::http::Request::builder().method("POST").uri("/health");
//
//     let health_handler_route = OpenApiRouter::new().routes(routes![get_health_handler]);
//
//     let res = health_handler_route.oneshot(request).await.unwrap();
// }
