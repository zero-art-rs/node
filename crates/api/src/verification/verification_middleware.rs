use crate::Container;
use crate::domains::art::transport::http::{
    DeleteChatQuery, GetChangesQuery, GetInitialARTQuery, InitChatRequest, UpdateKeyRequest,
};
use crate::domains::centrifugo::transport::http::AuthRequest;
use crate::domains::messenger::transport::http::SendMessageRequest;
use crate::errors::ApiError;
use crate::verification::{
    ArtUpdateRequestHelper, GetInitialARTQueryHelper, PreviousRootKnowledgeProofHelper,
    RootKnowledgeProofHelper, VerifyOwnershipQueryHelper,
};
use axum::extract::State;
use axum::middleware::Next;
use axum_core::body::Body;
use axum_core::extract::Request;
use axum_core::response::{IntoResponse, Response};
use std::sync::Arc;
use tracing::error;
use types::callback_wrappers::ProofVerifierResult;

pub async fn verification_middleware(
    State(state): State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Response {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    let new_request = Request::from_parts(parts, Body::from(bytes.clone()));
    let b = bytes.as_ref();

    if let Ok(_) = serde_json::from_slice::<AuthRequest>(b) {
        return next.run(new_request).await;
    }

    if let Ok(proof_helper) = RootKnowledgeProofHelper::try_from(b) {
        return match proof_helper.verify(state.clone()).await {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(proof_helper) = PreviousRootKnowledgeProofHelper::try_from(b) {
        return match proof_helper.verify(state.clone()).await {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(_) = serde_json::from_slice::<DeleteChatQuery>(b) {
        return next.run(new_request).await;
    }

    if let Ok(_) = serde_json::from_slice::<InitChatRequest>(b) {
        return next.run(new_request).await;
    }

    if let Ok(helper) = ArtUpdateRequestHelper::try_from(b) {
        return match helper.verify(state.clone()).await {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(helper) = GetInitialARTQueryHelper::try_from(b) {
        return match helper.verify(state.clone()).await {
            Ok(_) => next.run(new_request).await,
            Err(err) => ApiError::Unauthorized(err.to_string()).into_response(),
        };
    }

    if let Ok(helper) = VerifyOwnershipQueryHelper::try_from(b) {
        return match helper.verify(state.clone()).await {
            Ok(_) => next.run(new_request).await,
            Err(err) => ApiError::Unauthorized(err.to_string()).into_response(),
        };
    }

    error!("Unknown request occurred");
    // ApiError::BadRequest("Unknown request".to_string()).into_response()
    next.run(new_request).await
}
