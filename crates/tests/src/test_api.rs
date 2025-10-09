#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::routing::get;
    use axum_test::TestServer;

    #[tokio::test]
    async fn it_should_be_healthy() {
        // Build an application with a route.
        let app = Router::new().route(&"/health", get(async || "healthy"));

        // Run the application for testing.
        let server = TestServer::new(app).unwrap();

        // Get the request.
        let response = server.get("/health").await;

        // Assertions.
        response.assert_status_ok();
        response.assert_text("healthy");
    }
}
