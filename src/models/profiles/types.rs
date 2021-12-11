use chrono::{DateTime, Utc};
use postgres_types::{
    FromSql, ToSql, IsNull, Type, Json,
    accepts, to_sql_checked,
    private::BytesMut,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::activitypub::views::get_actor_url;
use crate::errors::{ConversionError, ValidationError};
use crate::utils::html::clean_html;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtraField {
    pub name: String,
    pub value: String,
    pub value_source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtraFields(pub Vec<ExtraField>);

impl ExtraFields {
    pub fn unpack(self) -> Vec<ExtraField> {
        let Self(extra_fields) = self;
        extra_fields
    }
}

type SqlError = Box<dyn std::error::Error + Sync + Send>;

impl<'a> FromSql<'a> for ExtraFields {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, SqlError> {
        let Json(json_value) = Json::<Value>::from_sql(ty, raw)?;
        let fields: Self = serde_json::from_value(json_value)?;
        Ok(fields)
    }
    accepts!(JSON,JSONB);
}

impl ToSql for ExtraFields {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, SqlError> {
        let value = serde_json::to_value(self)?;
        Json(value).to_sql(ty, out)
    }

    accepts!(JSON, JSONB);
    to_sql_checked!();
}

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
    pub extra_fields: ExtraFields,
    pub follower_count: i32,
    pub following_count: i32,
    pub post_count: i32,
    pub created_at: DateTime<Utc>,
    pub actor_json: Option<Value>,
}

impl DbActorProfile {
    pub fn is_local(&self) -> bool {
        self.actor_json.is_none()
    }

    pub fn actor_id(&self, instance_url: &str) -> Result<String, ConversionError> {
        let actor_id = match self.actor_json {
            Some(ref actor_value) => {
                actor_value["id"].as_str()
                    .ok_or(ConversionError)?
                    .to_string()
            },
            None => get_actor_url(instance_url, &self.username),
        };
        Ok(actor_id)
    }

    pub fn actor_url(&self, instance_url: &str) -> Result<String, ConversionError> {
        if let Some(ref actor_value) = self.actor_json {
            if let Some(actor_url) = actor_value["url"].as_str() {
                return Ok(actor_url.to_string());
            };
        };
        self.actor_id(instance_url)
    }

    pub fn actor_address(&self, instance_host: &str) -> String {
        if self.is_local() {
            format!("{}@{}", self.acct, instance_host)
        } else {
            self.acct.clone()
        }
    }
}

#[cfg(test)]
impl Default for DbActorProfile {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            username: "".to_string(),
            acct: "".to_string(),
            display_name: None,
            bio: None,
            bio_source: None,
            avatar_file_name: None,
            banner_file_name: None,
            extra_fields: ExtraFields(vec![]),
            follower_count: 0,
            following_count: 0,
            post_count: 0,
            created_at: Utc::now(),
            actor_json: None,
        }
    }
}

pub struct ProfileCreateData {
    pub username: String,
    pub display_name: Option<String>,
    pub acct: String,
    pub bio: Option<String>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub extra_fields: Vec<ExtraField>,
    pub actor: Option<Value>,
}

pub struct ProfileUpdateData {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub bio_source: Option<String>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub extra_fields: Vec<ExtraField>,
}

impl ProfileUpdateData {
    pub fn clean(&mut self) -> Result<(), ValidationError> {
        // Validate and clean bio
        self.bio = self.bio.as_ref().map(|val| clean_html(val));
        // Clean extra fields and remove fields with empty labels
        self.extra_fields = self.extra_fields.iter().cloned()
            .map(|mut field| {
                field.name = field.name.trim().to_string();
                field.value = clean_html(&field.value);
                field
            })
            .filter(|field| !field.name.is_empty())
            .collect();
        // Validate extra fields
        if self.extra_fields.len() >= 10 {
            return Err(ValidationError("at most 10 fields are allowed"));
        }
        let mut unique_labels: Vec<String> = self.extra_fields.iter()
            .map(|field| field.name.clone()).collect();
        unique_labels.sort();
        unique_labels.dedup();
        if unique_labels.len() < self.extra_fields.len() {
            return Err(ValidationError("duplicate labels"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    const INSTANCE_HOST: &str = "example.com";

    #[test]
    fn test_local_actor_address() {
        let local_profile = DbActorProfile {
            acct: "user".to_string(),
            actor_json: None,
            ..Default::default()
        };
        assert_eq!(
            local_profile.actor_address(INSTANCE_HOST),
            "user@example.com",
        );
    }

    #[test]
    fn test_remote_actor_address() {
        let remote_profile = DbActorProfile {
            acct: "test@remote.com".to_string(),
            actor_json: Some(json!({"id": "https://test"})),
            ..Default::default()
        };
        assert_eq!(
            remote_profile.actor_address(INSTANCE_HOST),
            remote_profile.acct,
        );
    }
}
