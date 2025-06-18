use std::sync::Arc;

use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{container::Container, domains::auth::service::Claims, errors::ApiError};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AuthRequest {
    pub public_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AuthResponse {
    pub token: String,
    pub expires_in: i64,
}

#[utoipa::path(
    post,
    path = "/auth",
    tag = "Authentication",
    request_body = AuthRequest,
    responses(
        (status = 200, description = "Authentication successful", body = AuthResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Invalid public key"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn authenticate(
    State(container): State<Arc<Container>>,
    Json(request): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let auth_service = &container.auth_service;

    // Authenticate the public key (mock implementation)
    let is_valid = auth_service
        .authenticate_public_key(&request.public_key)
        .await?;

    if !is_valid {
        return Err(ApiError::Unauthorized("Invalid public key".to_string()));
    }

    let token = auth_service.generate_token(request.public_key).await?;

    let response = AuthResponse {
        token,
        expires_in: 24 * 60 * 60, // 24 hours in seconds
    };

    Ok(Json(response))
}

pub async fn jwt_middleware(
    State(container): State<Arc<Container>>,
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| {
            if header.starts_with("Bearer ") {
                Some(&header[7..])
            } else {
                None
            }
        });

    let token = match auth_header {
        Some(token) => token,
        None => {
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Missing Authorization header"})),
            )
                .into_response());
        }
    };

    let auth_service = &container.auth_service;

    let claims = match auth_service.validate_token(token).await {
        Ok(claims) => claims,
        Err(_) => {
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Invalid token"})),
            )
                .into_response());
        }
    };

    request.extensions_mut().insert(claims);

    Ok(next.run(request).await)
}

// Extractor for getting authenticated user claims from request
#[derive(Clone)]
pub struct AuthenticatedUser(pub Claims);

impl<S> axum::extract::FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Claims>()
            .cloned()
            .map(AuthenticatedUser)
            .ok_or_else(|| ApiError::Unauthorized("Missing authentication claims".to_string()))
    }
}
