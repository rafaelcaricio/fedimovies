use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct AuthorizationRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct AuthorizationQueryParams {
    pub response_type: String,
    pub client_id: Uuid,
    pub redirect_uri: String,
    pub scope: String,
}

#[derive(Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,

    // Required if grant type is "authorization_code"
    pub code: Option<String>,

    // Required if grant type is "password" or "eip4361"
    pub username: Option<String>,
    pub wallet_address: Option<String>,
    // Required only with "password" and "ethereum" grant types
    pub password: Option<String>,
    // EIP4361 message and signature
    pub message: Option<String>,
    pub signature: Option<String>,
}

/// https://docs.joinmastodon.org/entities/token/
#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
    pub created_at: i64,
}

impl TokenResponse {
    pub fn new(access_token: String, created_at: i64) -> Self {
        Self {
            access_token,
            token_type: "Bearer".to_string(),
            scope: "read write follow".to_string(),
            created_at,
        }
    }
}

#[derive(Deserialize)]
pub struct RevocationRequest {
    pub token: String,
}
