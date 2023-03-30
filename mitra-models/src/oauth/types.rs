use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

#[derive(FromSql)]
#[postgres(name = "oauth_application")]
pub struct DbOauthApp {
    pub id: i32,
    pub app_name: String,
    pub website: Option<String>,
    pub scopes: String,
    pub redirect_uri: String,
    pub client_id: Uuid,
    pub client_secret: String,
    pub created_at: DateTime<Utc>,
}

#[cfg_attr(test, derive(Default))]
pub struct DbOauthAppData {
    pub app_name: String,
    pub website: Option<String>,
    pub scopes: String,
    pub redirect_uri: String,
    pub client_id: Uuid,
    pub client_secret: String,
}
