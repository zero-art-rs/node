use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use axum::Router;
use axum::routing::{get, post};
use axum_test::TestServer;
use mongodb::bson::doc;
use mongodb::Client;
use mongodb::options::ClientOptions;
use tokio::sync::{mpsc, RwLock};
use tracing::debug;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use api::{messenger_transport, art_transport, centrifugo_transport, Container, MessengerService, CentrifugoService, ARTService};
use proof_verifier::ProofVerifierSender;
use storage::DATABASE;
use crate::{
    user_test_model::UserTestModel,
    init_tracing_for_test
};

pub(crate) fn get_container(proof_verifier_sender: ProofVerifierSender) -> Container {
    let messenger_service = MessengerService::new();
    let centrifugo_service = CentrifugoService::new(
        String::from("tkE0hTS953BL3ETHeFyHGY3cAl78xyGCdPCtsGIX-oiyJ_Suz_ui_j3Gjrp8JP62Lq8tHCoih6rBMeUvfGPOvw"),
        Duration::new(86400, 0),
        vec![String::from("personal")],
    );
    let art_service = ARTService::new();

    Container {
        messenger_service: Arc::new(messenger_service),
        centrifugo_service: Arc::new(centrifugo_service),
        art_service: Arc::new(art_service),
        proof_verifier_sender,
        challenges: Arc::new(RwLock::new(HashSet::new())),
    }
}

pub(crate) async fn connect_to_test_db() -> eyre::Result<()> {
    let uri = String::from("mongodb://mongodb:27017/zkmsngr?replicaSet=rs0");
    let database_name = String::from("zkmsngr");

    let client_options = ClientOptions::parse(uri).await?;
    let client = Client::with_options(client_options)?;

    debug!("Try to connect to database ...");
    DATABASE
        .set(
            client
                .default_database()
                .unwrap_or(client.database(&database_name)),
        )
        .unwrap();
    debug!("Connection is successful");
    DATABASE
        .get()
        .unwrap()
        .run_command(doc! { "ping": 1 })
        .await?;

    Ok(())
}

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

#[tokio::test]
async fn test_init_group_endpoint() {
    let (proof_verifier_tx, proof_verifier_rx) = mpsc::channel(1000);
    let container = Arc::new(get_container(proof_verifier_tx));

    let router = Router::new()
        .route("/v1/group/{id}/frames", post(messenger_transport::send_frame))
        .with_state(container);

    let test_server = TestServer::new(router).unwrap();

    let user = UserTestModel::new(&test_server, 10);
}

// #[tokio::test]
// async fn test_add_member() -> eyre::Result<()> {
//     init_tracing_for_test();
//     connect_to_test_db().await?;
//
//     let (proof_verifier_tx, proof_verifier_rx) = mpsc::channel(1000);
//     let container = Arc::new(get_container(proof_verifier_tx));
//
//     let router = Router::new()
//         .route("/v1/group/{id}/frames", post(messenger_transport::send_frame))
//         .with_state(container);
//
//     let test_server = TestServer::new(router).unwrap();
//
//     let (mut user, _) = UserTestModel::new(&test_server, 10).await;
//     for _ in 0..10 {
//         user.add_member().await?;
//     }
//
//     Ok(())
// }
