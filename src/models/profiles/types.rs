use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde_json::Value;
use uuid::Uuid;

use crate::errors::ValidationError;
use crate::utils::html::clean_html;

#[derive(Clone, FromSql)]
#[postgres(name = "actor_profile")]
pub struct DbActorProfile {
    pub id: Uuid,
    pub username: String,
    pub acct: String,
    pub display_name: Option<String>,
    pub bio: Option<String>, // html
    pub bio_source: Option<String>, // plaintext or markdown
    pub avatar_file_name: Option<String>,
    pub banner_file_name: Option<String>,
    pub follower_count: i32,
    pub following_count: i32,
    pub post_count: i32,
    pub created_at: DateTime<Utc>,
    pub actor_json: Option<Value>,
}

pub struct ProfileCreateData {
    pub username: String,
    pub display_name: Option<String>,
    pub acct: String,
    pub bio: Option<String>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub actor: Option<Value>,
}

pub struct ProfileUpdateData {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub bio_source: Option<String>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
}

impl ProfileUpdateData {
    /// Validate and clean bio.
    pub fn clean(&mut self) -> Result<(), ValidationError> {
        self.bio = self.bio.as_ref().map(|val| clean_html(val));
        Ok(())
    }
}
