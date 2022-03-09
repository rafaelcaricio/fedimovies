use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ValidationError;
use crate::models::profiles::types::{
    DbActorProfile,
    ExtraField,
    ProfileUpdateData,
};
use crate::models::profiles::validators::validate_username;
use crate::models::users::types::{
    validate_local_username,
    User,
};
use crate::utils::files::{FileError, save_validated_b64_file, get_file_url};

#[derive(Serialize)]
pub struct AccountField {
    pub name: String,
    pub value: String,
}

/// https://docs.joinmastodon.org/entities/source/
#[derive(Serialize)]
pub struct Source {
    pub note: Option<String>,
    pub fields: Vec<AccountField>,
}

/// https://docs.joinmastodon.org/entities/account/
#[derive(Serialize)]
pub struct Account {
    pub id: Uuid,
    pub username: String,
    pub acct: String,
    pub url: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub note: Option<String>,
    pub avatar: Option<String>,
    pub header: Option<String>,
    pub fields: Vec<AccountField>,
    pub followers_count: i32,
    pub following_count: i32,
    pub statuses_count: i32,

    pub source: Option<Source>,

    pub wallet_address: Option<String>,
}

impl Account {
    pub fn from_profile(profile: DbActorProfile, instance_url: &str) -> Self {
        let profile_url = profile.actor_url(instance_url);
        let avatar_url = profile.avatar_file_name.as_ref()
            .map(|name| get_file_url(instance_url, name));
        let header_url = profile.banner_file_name.as_ref()
            .map(|name| get_file_url(instance_url, name));
        let fields = profile.extra_fields.unpack().into_iter()
            .map(|field| AccountField { name: field.name, value: field.value })
            .collect();
        Self {
            id: profile.id,
            username: profile.username,
            acct: profile.acct,
            url: profile_url,
            display_name: profile.display_name,
            created_at: profile.created_at,
            note: profile.bio,
            avatar: avatar_url,
            header: header_url,
            fields,
            followers_count: profile.follower_count,
            following_count: profile.following_count,
            statuses_count: profile.post_count,
            source: None,
            wallet_address: None,
        }
    }

    pub fn from_user(user: User, instance_url: &str) -> Self {
        let fields_sources = user.profile.extra_fields.clone()
            .unpack().into_iter()
            .map(|field| AccountField {
                name: field.name,
                value: field.value_source.unwrap_or(field.value),
            })
            .collect();
        let source = Source {
            note: user.profile.bio_source.clone(),
            fields: fields_sources,
        };
        let mut account = Self::from_profile(user.profile, instance_url);
        account.source = Some(source);
        account.wallet_address = user.wallet_address;
        account
    }
}

/// https://docs.joinmastodon.org/methods/accounts/
#[derive(Deserialize)]
pub struct AccountCreateData {
    pub username: String,
    pub password: Option<String>,

    pub message: Option<String>,
    pub signature: Option<String>,

    pub invite_code: Option<String>,
}

impl AccountCreateData {

    pub fn clean(&self) -> Result<(), ValidationError> {
        validate_username(&self.username)?;
        validate_local_username(&self.username)?;
        if self.password.is_none() && self.message.is_none() {
            return Err(ValidationError("password or EIP-4361 message is required"));
        };
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct AccountUpdateData {
    pub display_name: Option<String>,
    pub note: Option<String>,
    pub note_source: Option<String>,
    pub avatar: Option<String>,
    pub header: Option<String>,
    pub fields_attributes: Option<Vec<ExtraField>>,
}

fn process_b64_image_field_value(
    form_value: Option<String>,
    db_value: Option<String>,
    output_dir: &Path,
) -> Result<Option<String>, FileError> {
    let maybe_file_name = match form_value {
        Some(b64_data) => {
            if b64_data.is_empty() {
                // Remove file
                None
            } else {
                // Decode and save file
                let (file_name, _) = save_validated_b64_file(
                    &b64_data, output_dir, "image/",
                )?;
                Some(file_name)
            }
        },
        // Keep current value
        None => db_value,
    };
    Ok(maybe_file_name)
}

impl AccountUpdateData {
    pub fn into_profile_data(
        self,
        current_avatar: &Option<String>,
        current_banner: &Option<String>,
        media_dir: &Path,
    ) -> Result<ProfileUpdateData, FileError> {
        let avatar = process_b64_image_field_value(
            self.avatar, current_avatar.clone(), media_dir,
        )?;
        let banner = process_b64_image_field_value(
            self.header, current_banner.clone(), media_dir,
        )?;
        let extra_fields = self.fields_attributes.unwrap_or(vec![]);
        let profile_data = ProfileUpdateData {
            display_name: self.display_name,
            bio: self.note,
            bio_source: self.note_source,
            avatar,
            banner,
            extra_fields,
            actor_json: None,
        };
        Ok(profile_data)
    }
}

// TODO: actix currently doesn't support parameter arrays
// https://github.com/actix/actix-web/issues/2044
#[derive(Deserialize)]
pub struct RelationshipQueryParams {
    #[serde(rename(deserialize = "id[]"))]
    pub id: Uuid,
}

#[derive(Serialize)]
pub struct RelationshipMap {
    pub id: Uuid, // target ID
    pub following: bool,
    pub followed_by: bool,
    pub requested: bool,
    pub subscription_to: bool,
    pub subscription_from: bool,
    pub showing_reblogs: bool,
    pub showing_replies: bool,
}

fn default_showing_reblogs() -> bool { true }

fn default_showing_replies() -> bool { true }

impl Default for RelationshipMap {
    fn default() -> Self {
        Self {
            id: Default::default(),
            following: false,
            followed_by: false,
            requested: false,
            subscription_to: false,
            subscription_from: false,
            showing_reblogs: default_showing_reblogs(),
            showing_replies: default_showing_replies(),
        }
    }
}

#[derive(Deserialize)]
pub struct FollowData {
    #[serde(default = "default_showing_reblogs")]
    pub reblogs: bool,
    #[serde(default = "default_showing_replies")]
    pub replies: bool,
}

fn default_page_size() -> i64 { 20 }

#[derive(Deserialize)]
pub struct StatusListQueryParams {
    #[serde(default)]
    pub pinned: bool,

    pub max_id: Option<Uuid>,

    #[serde(default = "default_page_size")]
    pub limit: i64,
}

fn default_follow_list_page_size() -> i64 { 40 }

#[derive(Deserialize)]
pub struct FollowListQueryParams {
    pub max_id: Option<i32>,

    #[serde(default = "default_follow_list_page_size")]
    pub limit: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_validate_account_create_data() {
        let account_data = AccountCreateData {
            username: "test".to_string(),
            password: None,
            message: None,
            signature: Some("test".to_string()),
            invite_code: None,
        };
        let error = account_data.clean().unwrap_err();
        assert_eq!(error.to_string(), "password or EIP-4361 message is required");
    }

    #[test]
    fn test_create_account_from_profile() {
        let profile = DbActorProfile {
            avatar_file_name: Some("test".to_string()),
            ..Default::default()
        };
        let account = Account::from_profile(profile, INSTANCE_URL);

        assert_eq!(
            account.avatar.unwrap(),
            format!("{}/media/test", INSTANCE_URL),
        );
        assert!(account.source.is_none());
        assert!(account.wallet_address.is_none());
    }

    #[test]
    fn test_create_account_from_user() {
        let bio_source = "test";
        let wallet_address = "0x1234";
        let profile = DbActorProfile {
            bio_source: Some(bio_source.to_string()),
            ..Default::default()
        };
        let user = User {
            wallet_address: Some(wallet_address.to_string()),
            profile,
            ..Default::default()
        };
        let account = Account::from_user(user, INSTANCE_URL);

        assert_eq!(
            account.source.unwrap().note.unwrap(),
            bio_source,
        );
        assert_eq!(
            account.wallet_address.unwrap(),
            wallet_address,
        );
    }
}
