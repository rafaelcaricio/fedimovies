use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateAppRequest {
    pub client_name: String,
    pub redirect_uris: String,
    pub scopes: String,
    pub website: Option<String>,
}

/// https://docs.joinmastodon.org/entities/Application/
#[derive(Serialize)]
pub struct OauthApp {
    pub name: String,
    pub website: Option<String>,
    pub redirect_uri: String,
    pub client_id: Option<Uuid>,
    pub client_secret: Option<String>,
}
