use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub username: String, // wallet address
    pub password: String,
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
