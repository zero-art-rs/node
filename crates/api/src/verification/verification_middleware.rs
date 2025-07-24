use crate::Container;
use crate::domains::art::transport::http::{
    AddMemberRequest, DeleteChatQuery, GetARTQuery, GetChangesQuery, GetInitialARTQuery,
    RemoveMemberRequest, UpdateKeyRequest,
};
use crate::domains::messenger::transport::http::{
    DeleteMessageQuery, GetMessageQuery, SendMessageRequest,
};
use crate::errors::ApiError;
use crate::verification::{
    ArtUpdateHelper, GetInitialARTHelper, RootKnowledgeHelper, VerificationError,
    VerifyOwnershipHelper,
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
        Err(err) => return ApiError::Unauthorized(err.to_string()).into_response(),
    };
    let query = parts.uri.query();
    let body_bytes = bytes.as_ref();

    let response = route_by_method_and_verify(
        parts.method.clone(),
        parts.uri.path(),
        state.clone(),
        body_bytes,
        query,
    )
    .await;

    match response {
        Ok(_) => {
            next.run(Request::from_parts(
                parts.clone(),
                Body::from(bytes.clone()),
            ))
            .await
        }
        Err(err) => {
            info!("Verification Failed: {}", err);
            err.into_response()
        }
    }
}

async fn route_by_method_and_verify(
    method: Method,
    url_path: &str,
    state: Arc<Container>,
    body_bytes: &[u8],
    query: Option<&str>,
) -> Result<(), ApiError> {
    match method {
        Method::POST => verify_post_request(state.clone(), url_path, body_bytes).await,
        Method::GET => {
            let query_bytes = match query {
                Some(query) => query.as_bytes(),
                None => &[],
            };
            verify_get_query(state.clone(), url_path, query_bytes).await
        }
        Method::DELETE => {
            let query_bytes = match query {
                Some(query) => query.as_bytes(),
                None => {
                    return Err(ApiError::BadRequest("Missing query string".to_string()));
                }
            };
            verify_delete_query(state.clone(), url_path, query_bytes).await
        }
        _ => Err(VerificationError::UnsupportedMethod),
    }
    .map_err(ApiError::from)
}

async fn verify_post_request(
    state: Arc<Container>,
    url_path: &str,
    body_bytes: &[u8],
) -> Result<(), VerificationError> {
    match url_path {
        // ChatOperations
        "/v1/messenger/add-member" => {
            ArtUpdateHelper::from(serde_json::from_slice::<AddMemberRequest>(body_bytes)?)
                .verify(state)
                .await
        }
        "/v1/messenger/remove-member" => {
            ArtUpdateHelper::from(serde_json::from_slice::<RemoveMemberRequest>(body_bytes)?)
                .verify(state)
                .await
        }
        "/v1/messenger/update-key" => {
            ArtUpdateHelper::from(serde_json::from_slice::<UpdateKeyRequest>(body_bytes)?)
                .verify(state)
                .await
        }
        // Messages
        "/v1/messenger/messages" => {
            RootKnowledgeHelper::from(serde_json::from_slice::<SendMessageRequest>(body_bytes)?)
                .verify(state)
                .await
        }
        _ => {
            error!("ERROR: Unknown request occurred");
            Err(VerificationError::UnknownEndpoint)
        }
    }
}

async fn verify_get_query(
    state: Arc<Container>,
    url_path: &str,
    query_bytes: &[u8],
) -> Result<(), VerificationError> {
    match url_path {
        // ChatOperations
        "/v1/messenger/art" => {
            RootKnowledgeHelper::from(serde_urlencoded::from_bytes::<GetARTQuery>(query_bytes)?)
                .verify(state)
                .await
        }
        "/v1/messenger/changes" => {
            RootKnowledgeHelper::from(serde_urlencoded::from_bytes::<GetChangesQuery>(
                query_bytes,
            )?)
            .verify(state)
            .await
        }
        "/v1/messenger/initial-art" => {
            GetInitialARTHelper::from(serde_urlencoded::from_bytes::<GetInitialARTQuery>(
                query_bytes,
            )?)
            .verify(state)
            .await
        }
        // Message
        "/v1/messenger/messages" => {
            RootKnowledgeHelper::from(serde_urlencoded::from_bytes::<GetMessageQuery>(
                query_bytes,
            )?)
            .verify(state)
            .await
        }
        _ => {
            error!("ERROR: Unknown request occurred");
            Err(VerificationError::UnknownEndpoint)
        }
    }
}

async fn verify_delete_query(
    state: Arc<Container>,
    url_path: &str,
    query_bytes: &[u8],
) -> Result<(), VerificationError> {
    match url_path {
        "/v1/messenger/chat" => {
            VerifyOwnershipHelper::from(serde_urlencoded::from_bytes::<DeleteChatQuery>(
                query_bytes,
            )?)
            .verify(state)
            .await
        }
        // Message
        "/v1/messenger/messages" => {
            RootKnowledgeHelper::from(serde_urlencoded::from_bytes::<DeleteMessageQuery>(
                query_bytes,
            )?)
            .verify(state)
            .await
        }
        // Unsupported case
        _ => {
            error!("ERROR: Unknown request occurred");
            Err(VerificationError::UnknownEndpoint)
        }
    }
}
