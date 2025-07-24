use crate::Container;
use crate::domains::art::transport::http::{
    AddMemberRequest, DeleteChatQuery, GetARTQuery, GetChangesQuery, GetInitialARTQuery,
    InitChatRequest, RemoveMemberRequest, UpdateKeyRequest,
};
use crate::domains::centrifugo::transport::http::AuthRequest;
use crate::domains::messenger::transport::http::{
    DeleteMessageQuery, GetMessageQuery, SendMessageRequest,
};
use crate::errors::ApiError;
use crate::verification::{
    ArtUpdateHelper, GetInitialARTHelper, RootKnowledgeHelper, VerifyOwnershipHelper,
};
use axum::extract::State;
use axum::http::Method;
use axum::middleware::Next;
use axum_core::body::Body;
use axum_core::extract::Request;
use axum_core::response::{IntoResponse, Response};
use std::sync::Arc;
use tracing::{error, info};

pub async fn verification_middleware(
    State(state): State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Response {
    info!("Incoming request: {} {}", request.method(), request.uri());

    let (parts, body) = request.into_parts();

    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(err) => return ApiError::BadRequest(err.to_string()).into_response(),
    };
    let query = parts.uri.query();
    let new_request = Request::from_parts(parts.clone(), Body::from(bytes.clone()));
    let body_bytes = bytes.as_ref();

    match parts.method {
        Method::POST => {
            match verify_post_request(state.clone(), parts.uri.path(), body_bytes).await {
                Ok(_) => next.run(new_request).await,
                Err(err) => ApiError::Unauthorized(err.to_string()).into_response(),
            }
        }
        Method::GET => {
            let query_bytes = match query {
                Some(query) => query.as_bytes(),
                None => &[],
            };
            match verify_get_query(state.clone(), parts.uri.path(), query_bytes).await {
                Ok(_) => next.run(new_request).await,
                Err(err) => ApiError::Unauthorized(err.to_string()).into_response(),
            }
        }
        Method::DELETE => {
            let query_bytes = match query {
                Some(query) => query.as_bytes(),
                None => {
                    return ApiError::BadRequest("Missing query string".to_string())
                        .into_response();
                }
            };
            match verify_delete_query(state.clone(), parts.uri.path(), query_bytes).await {
                Ok(_) => next.run(new_request).await,
                Err(err) => ApiError::Unauthorized(err.to_string()).into_response(),
            }
        }
        _ => ApiError::BadRequest("Unsupported method".to_string()).into_response(),
    }
}

async fn verify_post_request(
    state: Arc<Container>,
    url_path: &str,
    body_bytes: &[u8],
) -> Result<(), ApiError> {
    match url_path {
        // Centrifugo
        "/centrifugo/auth" => {
            info!("No AuthRequest verification required");
            Ok(())
        }
        // ChatOperations
        "/v1/messenger/add-member" => {
            match serde_json::from_slice::<AddMemberRequest>(body_bytes) {
                Ok(request) => ArtUpdateHelper::from(request).verify(state).await,
                Err(err) => Err(ApiError::BadRequest(err.to_string())),
            }
        }
        "/v1/messenger/init-chat" => {
            info!("No InitChat verification required");
            Ok(())
        }
        "/v1/messenger/remove-member" => {
            match serde_json::from_slice::<RemoveMemberRequest>(body_bytes) {
                Ok(request) => ArtUpdateHelper::from(request).verify(state).await,
                Err(err) => Err(ApiError::BadRequest(err.to_string())),
            }
        }
        "/v1/messenger/update-key" => {
            match serde_json::from_slice::<UpdateKeyRequest>(body_bytes) {
                Ok(request) => ArtUpdateHelper::from(request).verify(state).await,
                Err(err) => Err(ApiError::BadRequest(err.to_string())),
            }
        }
        // Messages
        "/v1/messenger/messages" => {
            match serde_json::from_slice::<SendMessageRequest>(body_bytes) {
                Ok(query) => RootKnowledgeHelper::from(query).verify(state).await,
                Err(err) => Err(ApiError::BadRequest(err.to_string())),
            }
        }
        _ => {
            error!("ERROR: Unknown request occurred");
            Err(ApiError::BadRequest("Unknown request".to_string()))
        }
    }
}

async fn verify_get_query(
    state: Arc<Container>,
    url_path: &str,
    query_bytes: &[u8],
) -> Result<(), ApiError> {
    match url_path {
        // Swagger
        "/swagger"
        | "/favicon.ico"
        | "/swagger/"
        | "/swagger/swagger-ui.css"
        | "/swagger/index.css"
        | "/swagger/swagger-ui-bundle.js"
        | "/swagger/swagger-ui-standalone-preset.js"
        | "/swagger/swagger-initializer.js"
        | "/swagger/favicon-32x32.png"
        | "/spec.json" => {
            info!("No verification required for swagger");
            Ok(())
        }
        // Health
        "/health" => {
            info!("No health request verification required");
            Ok(())
        }
        // ChatOperations
        "/v1/messenger/challenge" => {
            info!("No verification required for challenge retrieval");
            Ok(())
        }
        "/v1/messenger/art" => match serde_urlencoded::from_bytes::<GetARTQuery>(query_bytes) {
            Ok(query) => RootKnowledgeHelper::from(query).verify(state).await,
            Err(err) => Err(ApiError::BadRequest(err.to_string())),
        },
        "/v1/messenger/changes" => {
            match serde_urlencoded::from_bytes::<GetChangesQuery>(query_bytes) {
                Ok(query) => RootKnowledgeHelper::from(query).verify(state).await,
                Err(err) => Err(ApiError::BadRequest(err.to_string())),
            }
        }
        "/v1/messenger/initial-art" => {
            match serde_urlencoded::from_bytes::<GetInitialARTQuery>(query_bytes) {
                Ok(query) => GetInitialARTHelper::from(query).verify(state).await,
                Err(err) => Err(ApiError::BadRequest(err.to_string())),
            }
        }
        // Message
        "/v1/messenger/messages" => {
            match serde_urlencoded::from_bytes::<GetMessageQuery>(query_bytes) {
                Ok(query) => RootKnowledgeHelper::from(query).verify(state).await,
                Err(err) => Err(ApiError::BadRequest(err.to_string())),
            }
        }
        _ => {
            error!("ERROR: Unknown request occurred");
            Err(ApiError::BadRequest("Unknown request".to_string()))
        }
    }
}

async fn verify_delete_query(
    state: Arc<Container>,
    url_path: &str,
    query_bytes: &[u8],
) -> Result<(), ApiError> {
    match url_path {
        "/v1/messenger/chat" => {
            match serde_urlencoded::from_bytes::<DeleteChatQuery>(query_bytes) {
                Ok(query) => VerifyOwnershipHelper::from(query).verify(state).await,
                Err(err) => Err(ApiError::BadRequest(err.to_string())),
            }
        }
        // Message
        "/v1/messenger/messages" => {
            match serde_urlencoded::from_bytes::<DeleteMessageQuery>(query_bytes) {
                Ok(query) => RootKnowledgeHelper::from(query).verify(state).await,
                Err(err) => Err(ApiError::BadRequest(err.to_string())),
            }
        }
        // Unsupported case
        _ => {
            error!("ERROR: Unknown request occurred");
            Err(ApiError::BadRequest("Unknown request".to_string()))
        }
    }
}
