use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::activitypub::actor::Actor;
use crate::activitypub::views::get_actor_url;
use crate::database::json_macro::{json_from_sql, json_to_sql};
use crate::errors::ValidationError;
use crate::ethereum::identity::DidPkh;
use super::validators::{
    validate_username,
    validate_display_name,
    clean_bio,
    clean_extra_fields,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdentityProof {
    pub issuer: DidPkh,
    pub proof_type: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdentityProofs(pub Vec<IdentityProof>);

impl IdentityProofs {
    pub fn into_inner(self) -> Vec<IdentityProof> {
        let Self(identity_proofs) = self;
        identity_proofs
    }
}

json_from_sql!(IdentityProofs);
json_to_sql!(IdentityProofs);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtraField {
    pub name: String,
    pub value: String,
    pub value_source: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ExtraFields(pub Vec<ExtraField>);

impl ExtraFields {
    pub fn into_inner(self) -> Vec<ExtraField> {
        let Self(extra_fields) = self;
        extra_fields
    }
}

json_from_sql!(ExtraFields);
json_to_sql!(ExtraFields);

json_from_sql!(Actor);
json_to_sql!(Actor);

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
    pub identity_proofs: IdentityProofs,
    pub extra_fields: ExtraFields,
    pub follower_count: i32,
    pub following_count: i32,
    pub post_count: i32,
    pub created_at: DateTime<Utc>,
    pub actor_json: Option<Actor>,

    // auto-generated database fields
    pub actor_id: Option<String>,
}

impl DbActorProfile {
    pub fn is_local(&self) -> bool {
        self.actor_json.is_none()
    }

    pub fn actor_id(&self, instance_url: &str) -> String {
        // TODO: use actor_id field
        match self.actor_json {
            Some(ref actor) => actor.id.clone(),
            None => get_actor_url(instance_url, &self.username),
        }
    }

    /// Profile URL
    pub fn actor_url(&self, instance_url: &str) -> String {
        if let Some(ref actor) = self.actor_json {
            if let Some(ref actor_url) = actor.url {
                return actor_url.to_string();
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
            identity_proofs: IdentityProofs(vec![]),
            extra_fields: ExtraFields(vec![]),
            follower_count: 0,
            following_count: 0,
            post_count: 0,
            created_at: Utc::now(),
            actor_json: None,
            actor_id: None,
        }
    }
}

#[cfg_attr(test, derive(Default))]
pub struct ProfileCreateData {
    pub username: String,
    pub display_name: Option<String>,
    pub acct: String,
    pub bio: Option<String>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub identity_proofs: Vec<IdentityProof>,
    pub extra_fields: Vec<ExtraField>,
    pub actor_json: Option<Actor>,
}

impl ProfileCreateData {
    pub fn clean(&mut self) -> Result<(), ValidationError> {
        validate_username(&self.username)?;
        if let Some(display_name) = &self.display_name {
            validate_display_name(display_name)?;
        };
        if let Some(bio) = &self.bio {
            let cleaned_bio = clean_bio(bio, self.actor_json.is_some())?;
            self.bio = Some(cleaned_bio);
        };
        self.extra_fields = clean_extra_fields(&self.extra_fields)?;
        Ok(())
    }
}

pub struct ProfileUpdateData {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub bio_source: Option<String>,
    pub avatar: Option<String>,
    pub banner: Option<String>,
    pub identity_proofs: Vec<IdentityProof>,
    pub extra_fields: Vec<ExtraField>,
    pub actor_json: Option<Actor>,
}

impl ProfileUpdateData {
    pub fn clean(&mut self) -> Result<(), ValidationError> {
        if let Some(display_name) = &self.display_name {
            validate_display_name(display_name)?;
        };
        // Validate and clean bio
        if let Some(bio) = &self.bio {
            let cleaned_bio = clean_bio(bio, self.actor_json.is_some())?;
            self.bio = Some(cleaned_bio);
        };
        // Clean extra fields and remove fields with empty labels
        self.extra_fields = clean_extra_fields(&self.extra_fields)?;
        Ok(())
    }
}

impl From<&DbActorProfile> for ProfileUpdateData {
    fn from(profile: &DbActorProfile) -> Self {
        let profile = profile.clone();
        Self {
            display_name: profile.display_name,
            bio: profile.bio,
            bio_source: profile.bio_source,
            avatar: profile.avatar_file_name,
            banner: profile.banner_file_name,
            identity_proofs: profile.identity_proofs.into_inner(),
            extra_fields: profile.extra_fields.into_inner(),
            actor_json: profile.actor_json,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::activitypub::actor::Actor;
    use super::*;

    const INSTANCE_HOST: &str = "example.com";

    #[test]
    fn test_identity_proof_serialization() {
        let json_data = r#"{"issuer":"did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a","proof_type":"ethereum-eip191-00","value":"dbfe"}"#;
        let proof: IdentityProof = serde_json::from_str(json_data).unwrap();
        assert_eq!(proof.issuer.address, "0xb9c5714089478a327f09197987f16f9e5d936e8a");
        let serialized = serde_json::to_string(&proof).unwrap();
        assert_eq!(serialized, json_data);
    }

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
            actor_json: Some(Actor {
                id: "https://test".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            remote_profile.actor_address(INSTANCE_HOST),
            remote_profile.acct,
        );
    }
}
