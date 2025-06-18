use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ApiError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub public_key: String,
    pub exp: i64,
    pub iat: i64,
    pub jti: String,
}

#[derive(Clone)]
pub struct AuthService {
    jwt_secret: String,
    // Add db in the future
}

impl AuthService {
    pub fn new(jwt_secret: String) -> Self {
        Self { jwt_secret }
    }

    pub async fn generate_token(&self, public_key: String) -> Result<String, ApiError> {
        let now = Utc::now();
        let expiration = now + Duration::hours(24); // Token valid for 24 hours

        let claims = Claims {
            public_key,
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

    pub async fn authenticate_public_key(&self, public_key: &str) -> Result<bool, ApiError> {
        Ok(!public_key.is_empty())
    }
}
