use crate::Container;
use axum::extract::State;
use axum::http::Method;
use axum::middleware::Next;
use axum_core::body::Body;
use axum_core::extract::Request;
use axum_core::response::{IntoResponse, Response};
use proof_verifier::VerificationHelper;
use std::sync::Arc;
use tracing::{error, info};
use types::art_schemas::{
    AddMemberRequest, DeleteChatQuery, GetARTQuery, GetChangesQuery, GetInitialARTQuery,
    RemoveMemberRequest, UpdateKeyRequest,
};
use types::errors::ApiError;
use types::errors::VerificationError;
use types::messenger_schemas::{DeleteMessageQuery, GetMessageQuery, SendMessageRequest};

pub async fn verification_middleware(
    State(state): State<Arc<Container>>,
    request: Request,
    next: Next,
) -> Response {
    info!(
        "Incoming verification request: {} {}",
        request.method(),
        request.uri()
    );

    let (parts, body) = request.into_parts();

    let bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(err) => return ApiError::Unauthorized(err.to_string()).into_response(),
    };

    let response = route_by_method_and_verify(
        parts.method.clone(),
        parts.uri.path(),
        state.clone(),
        bytes.as_ref(),
        parts.uri.query(),
    )
    .await;

    match response {
        Ok(_) => {
            info!("verification completed successfully");
            next.run(Request::from_parts(
                parts.clone(),
                Body::from(bytes.clone()),
            ))
            .await
        }
        Err(err) => {
            error!("Verification Failed: {}", err);
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
            let query_bytes = query
                .ok_or(ApiError::from(VerificationError::MissingQuery))?
                .as_bytes();
            verify_get_query(state.clone(), url_path, query_bytes).await
        }
        Method::DELETE => {
            let query_bytes = query
                .ok_or(ApiError::from(VerificationError::MissingQuery))?
                .as_bytes();
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
    let (helper, art) = match url_path {
        // ChatOperations
        "/v1/messenger/add-member" => {
            let helper =
                VerificationHelper::from(serde_json::from_slice::<AddMemberRequest>(body_bytes)?);
            let art = state.art_service.get_art(&helper.chat_id, None).await?.art;
            (helper, art)
        }
        "/v1/messenger/remove-member" => {
            let helper = VerificationHelper::from(serde_json::from_slice::<RemoveMemberRequest>(
                body_bytes,
            )?);
            let art = state.art_service.get_art(&helper.chat_id, None).await?.art;
            (helper, art)
        }
        "/v1/messenger/update-key" => {
            let helper =
                VerificationHelper::from(serde_json::from_slice::<UpdateKeyRequest>(body_bytes)?);
            let art = state.art_service.get_art(&helper.chat_id, None).await?.art;
            (helper, art)
        }
        // Messages
        "/v1/messenger/messages" => {
            let helper =
                VerificationHelper::from(serde_json::from_slice::<SendMessageRequest>(body_bytes)?);
            let art = state
                .art_service
                .get_art(&helper.chat_id, helper.sequence_number)
                .await?
                .art;
            (helper, art)
        }
        _ => {
            error!("ERROR: Unknown request occurred");
            return Err(VerificationError::UnknownEndpoint);
        }
    };

    helper
        .verify(&art, &state.proof_verifier_sender, &state.challenges)
        .await
}

async fn verify_get_query(
    state: Arc<Container>,
    url_path: &str,
    query_bytes: &[u8],
) -> Result<(), VerificationError> {
    let (helper, art) = match url_path {
        // ChatOperations
        "/v1/messenger/art" => {
            let helper =
                VerificationHelper::from(serde_urlencoded::from_bytes::<GetARTQuery>(query_bytes)?);
            let art = state
                .art_service
                .get_previous_art(&helper.chat_id, helper.sequence_number)
                .await?
                .art;
            (helper, art)
        }
        "/v1/messenger/changes" => {
            let helper = VerificationHelper::from(serde_urlencoded::from_bytes::<GetChangesQuery>(
                query_bytes,
            )?);
            let art = state
                .art_service
                .get_previous_art(&helper.chat_id, helper.sequence_number)
                .await?
                .art;
            (helper, art)
        }
        "/v1/messenger/initial-art" => {
            let helper = VerificationHelper::from(serde_urlencoded::from_bytes::<
                GetInitialARTQuery,
            >(query_bytes)?);
            let art = state
                .art_service
                .get_initial_art(&helper.chat_id)
                .await?
                .art;
            (helper, art)
        }
        // Message
        "/v1/messenger/messages" => {
            let helper = VerificationHelper::from(serde_urlencoded::from_bytes::<GetMessageQuery>(
                query_bytes,
            )?);
            let art = state.art_service.get_art(&helper.chat_id, None).await?.art;
            (helper, art)
        }
        _ => {
            error!("ERROR: Unknown request occurred");
            return Err(VerificationError::UnknownEndpoint);
        }
    };

    helper
        .verify(&art, &state.proof_verifier_sender, &state.challenges)
        .await
}

async fn verify_delete_query(
    state: Arc<Container>,
    url_path: &str,
    query_bytes: &[u8],
) -> Result<(), VerificationError> {
    let (helper, art) = match url_path {
        "/v1/messenger/chat" => {
            let helper = VerificationHelper::from(serde_urlencoded::from_bytes::<DeleteChatQuery>(
                query_bytes,
            )?);
            let art = state.art_service.get_art(&helper.chat_id, None).await?.art;
            (helper, art)
        }
        // Message
        "/v1/messenger/messages" => {
            let helper = VerificationHelper::from(serde_urlencoded::from_bytes::<
                DeleteMessageQuery,
            >(query_bytes)?);
            let art = state.art_service.get_art(&helper.chat_id, None).await?.art;
            (helper, art)
        }
        _ => {
            error!("ERROR: Unknown request occurred");
            return Err(VerificationError::UnknownEndpoint);
        }
    };

    helper
        .verify(&art, &state.proof_verifier_sender, &state.challenges)
        .await
}
