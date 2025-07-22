use std::time::Duration;

use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{domains::centrifugo::transport::http::AuthRequest, errors::ApiError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub channels: Vec<String>,
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
}

#[derive(Clone)]
pub struct CentrifugoService {
    jwt_secret: String,
    token_ttl: Duration,
    namespaces: Vec<String>, // Add db in the future
}

impl CentrifugoService {
    pub fn new(jwt_secret: String, token_ttl: Duration, namespaces: Vec<String>) -> Self {
        Self {
            jwt_secret,
            token_ttl,
            namespaces,
        }
    }

    pub fn token_ttl(&self) -> Duration {
        self.token_ttl
    }

    pub async fn generate_token(&self, request: &AuthRequest) -> Result<String, ApiError> {
        let now = Utc::now();
        let expiration = now + self.token_ttl;

        let mut channels = Vec::new();
        for namespace in &self.namespaces {
            for chat_id in &request.chat_ids {
                channels.push(format!("{}:{}", namespace, chat_id));
            }
        }

        let claims = Claims {
            sub: hex::encode(request.public_key.clone()),
            channels,
            exp: expiration.timestamp(),
            iat: now.timestamp(),
            jti: Uuid::new_v4().to_string(),
        };

        let header = Header::new(Algorithm::HS256);
        let encoding_key = EncodingKey::from_secret(self.jwt_secret.as_ref());

        encode(&header, &claims, &encoding_key)
            .map_err(|e| ApiError::InternalServerError(format!("Failed to generate JWT: {}", e)))
    }

    pub async fn validate_token(&self, token: &str) -> Result<Claims, ApiError> {
        let decoding_key = DecodingKey::from_secret(self.jwt_secret.as_ref());
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        decode::<Claims>(token, &decoding_key, &validation)
            .map(|token_data| token_data.claims)
            .map_err(|e| ApiError::Unauthorized(format!("Invalid token: {}", e)))
    }
}
