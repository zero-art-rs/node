use crate::Container;
use crate::domains::art::transport::http::{
    GetChangesQuery, GetInitialARTQuery, InitChatRequest, UpdateKeyRequest,
};
use crate::domains::centrifugo::transport::http::AuthRequest;
use crate::errors::ApiError;
use crate::verification::{
    ArtUpdateHelper, GetInitialARTHelper,
    RootKnowledgeHelper, VerifyOwnershipHelper,
};
use axum::extract::State;
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
        Err(err) => return ApiError::BadRequest(err.to_string()).into_response()
    };
    let query = parts.uri.query();
    let new_request = Request::from_parts(parts.clone(), Body::from(bytes.clone()));
    let b = bytes.as_ref();

    if let Ok(_) = serde_json::from_slice::<AuthRequest>(b) {
        return next.run(new_request).await;
    }

    if let Ok(_) = serde_json::from_slice::<InitChatRequest>(b) {
        return next.run(new_request).await;
    }

    if let Ok(proof_helper) = RootKnowledgeHelper::try_from(b) {
        return match proof_helper.verify(state.clone()).await {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }

    if let Ok(helper) = ArtUpdateHelper::try_from(b) {
        return match helper.verify(state.clone()).await {
            Ok(_) => next.run(new_request).await,
            Err(err) => err.into_response(),
        };
    }
    
    if let Some(query) = query {
        if let Ok(helper) = GetInitialARTHelper::try_from(query.as_bytes()) {
            return match helper.verify(state.clone()).await {
                Ok(_) => next.run(new_request).await,
                Err(err) => err.into_response(),
            };
        }

        if let Ok(helper) = VerifyOwnershipHelper::try_from(query.as_bytes()) {
            return match helper.verify(state.clone()).await {
                Ok(_) => next.run(new_request).await,
                Err(err) => ApiError::Unauthorized(err.to_string()).into_response(),
            };
        }
        
        if let Ok(helper) = RootKnowledgeHelper::try_from(query.as_bytes()) {
            return match helper.verify(state.clone()).await {
                Ok(_) => next.run(new_request).await,
                Err(err) => err.into_response(),
            };
        }
    }

    error!("ERROR: Unknown request occurred");
    ApiError::BadRequest("Unknown request".to_string()).into_response()
}
