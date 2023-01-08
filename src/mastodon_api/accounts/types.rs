use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::{HttpError, ValidationError};
use crate::identity::did::Did;
use crate::mastodon_api::pagination::PageSize;
use crate::mastodon_api::uploads::{save_b64_file, UploadError};
use crate::models::profiles::types::{
    DbActorProfile,
    ExtraField,
    PaymentOption,
    ProfileImage,
    ProfileUpdateData,
};
use crate::models::profiles::validators::validate_username;
use crate::models::subscriptions::types::Subscription;
use crate::models::users::types::{
    validate_local_username,
    User,
};
use crate::utils::{
    files::get_file_url,
    markdown::markdown_basic_to_html,
};

/// https://docs.joinmastodon.org/entities/field/
#[derive(Serialize)]
pub struct AccountField {
    pub name: String,
    pub value: String,
    verified_at: Option<DateTime<Utc>>,
}

/// Contains only public information
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AccountPaymentOption {
    Link { name: String, href: String },
    EthereumSubscription,
    MoneroSubscription { price: u64 },
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
    pub locked: bool,
    pub identity_proofs: Vec<AccountField>,
    pub payment_options: Vec<AccountPaymentOption>,
    pub fields: Vec<AccountField>,
    pub followers_count: i32,
    pub following_count: i32,
    pub subscribers_count: i32,
    pub statuses_count: i32,

    pub source: Option<Source>,
}

impl Account {
    pub fn from_profile(profile: DbActorProfile, instance_url: &str) -> Self {
        let profile_url = profile.actor_url(instance_url);
        let avatar_url = profile.avatar
            .map(|image| get_file_url(instance_url, &image.file_name));
        let header_url = profile.banner
            .map(|image| get_file_url(instance_url, &image.file_name));
        let is_locked = profile.actor_json
            .map(|actor| actor.manually_approves_followers)
            .unwrap_or(false);

        let mut identity_proofs = vec![];
        for proof in profile.identity_proofs.into_inner() {
            let (field_name, field_value) = match proof.issuer {
                Did::Key(did_key) => {
                    ("Key".to_string(), did_key.key_multibase())
                },
                Did::Pkh(did_pkh) => {
                    let field_name = did_pkh.currency()
                        .map(|currency| currency.field_name())
                        .unwrap_or("$".to_string());
                    (field_name, did_pkh.address)
                }
            };
            let field = AccountField {
                name: field_name,
                value: field_value,
                // Use current time because DID proofs are always valid
                verified_at: Some(Utc::now()),
            };
            identity_proofs.push(field);
        };

        let mut extra_fields = vec![];
        for extra_field in profile.extra_fields.into_inner() {
            let field = AccountField {
                name: extra_field.name,
                value: extra_field.value,
                verified_at: None,
            };
            extra_fields.push(field);
        };

        let payment_options = profile.payment_options.into_inner()
            .into_iter()
            .map(|option| {
                match option {
                    PaymentOption::Link(link) => {
                        AccountPaymentOption::Link {
                            name: link.name,
                            href: link.href,
                        }
                    },
                    PaymentOption::EthereumSubscription(_) => {
                        AccountPaymentOption::EthereumSubscription
                    },
                    PaymentOption::MoneroSubscription(payment_info) => {
                        AccountPaymentOption::MoneroSubscription {
                            price: payment_info.price,
                        }
                    },
                }
            })
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
            locked: is_locked,
            identity_proofs,
            payment_options,
            fields: extra_fields,
            followers_count: profile.follower_count,
            following_count: profile.following_count,
            subscribers_count: profile.subscriber_count,
            statuses_count: profile.post_count,
            source: None,
        }
    }

    pub fn from_user(user: User, instance_url: &str) -> Self {
        let fields_sources = user.profile.extra_fields.clone()
            .into_inner().into_iter()
            .map(|field| AccountField {
                name: field.name,
                value: field.value_source.unwrap_or(field.value),
                verified_at: None,
            })
            .collect();
        let source = Source {
            note: user.profile.bio_source.clone(),
            fields: fields_sources,
        };
        let mut account = Self::from_profile(user.profile, instance_url);
        account.source = Some(source);
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
struct AccountFieldSource {
    name: String,
    value: String,
}

#[derive(Deserialize)]
pub struct AccountUpdateData {
    display_name: Option<String>,
    note: Option<String>,
    avatar: Option<String>,
    avatar_media_type: Option<String>,
    header: Option<String>,
    header_media_type: Option<String>,
    fields_attributes: Option<Vec<AccountFieldSource>>,
}

fn process_b64_image_field_value(
    form_value: Option<String>,
    form_media_type: Option<String>,
    db_value: Option<ProfileImage>,
    output_dir: &Path,
) -> Result<Option<ProfileImage>, UploadError> {
    let maybe_file_name = match form_value {
        Some(b64_data) => {
            if b64_data.is_empty() {
                // Remove file
                None
            } else {
                // Decode and save file
                let (file_name, media_type) = save_b64_file(
                    &b64_data,
                    form_media_type,
                    output_dir,
                    Some("image/"),
                )?;
                let image = ProfileImage {
                    file_name,
                    media_type: Some(media_type),
                };
                Some(image)
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
        profile: &DbActorProfile,
        media_dir: &Path,
    ) -> Result<ProfileUpdateData, HttpError> {
        let maybe_bio = if let Some(ref bio_source) = self.note {
            let bio = markdown_basic_to_html(bio_source)
                .map_err(|_| ValidationError("invalid markdown"))?;
            Some(bio)
        } else {
            None
        };
        let avatar = process_b64_image_field_value(
            self.avatar,
            self.avatar_media_type,
            profile.avatar.clone(),
            media_dir,
        )?;
        let banner = process_b64_image_field_value(
            self.header,
            self.header_media_type,
            profile.banner.clone(),
            media_dir,
        )?;
        let identity_proofs = profile.identity_proofs.inner().to_vec();
        let payment_options = profile.payment_options.inner().to_vec();
        let mut extra_fields = vec![];
        for field_source in self.fields_attributes.unwrap_or(vec![]) {
            let value = markdown_basic_to_html(&field_source.value)
                .map_err(|_| ValidationError("invalid markdown"))?;
            let extra_field = ExtraField {
                name: field_source.name,
                value: value,
                value_source: Some(field_source.value),
            };
            extra_fields.push(extra_field);
        };
        let profile_data = ProfileUpdateData {
            display_name: self.display_name,
            bio: maybe_bio,
            bio_source: self.note,
            avatar,
            banner,
            identity_proofs,
            payment_options,
            extra_fields,
            actor_json: None, // always None for local profiles
        };
        Ok(profile_data)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ActivityParams {
    Update { internal_activity_id: Uuid },
}

#[derive(Serialize)]
pub struct UnsignedActivity {
    pub params: ActivityParams,
    pub message: String, // canonical representation
}

#[derive(Deserialize)]
pub struct SignedActivity {
    pub params: ActivityParams,
    pub signer: String,
    pub signature: String,
}

#[derive(Deserialize)]
pub struct IdentityClaimQueryParams {
    pub proof_type: String,
    pub signer: String,
}

#[derive(Serialize)]
pub struct IdentityClaim {
    pub did: Did,
    pub claim: String,
}

#[derive(Deserialize)]
pub struct IdentityProofData {
    pub did: String,
    pub signature: String,
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
pub struct LookupAcctQueryParams {
    pub acct: String,
}

fn default_search_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct SearchAcctQueryParams {
    pub q: String,

    #[serde(default = "default_search_page_size")]
    pub limit: PageSize,
}

#[derive(Deserialize)]
pub struct SearchDidQueryParams {
    pub did: String,
}

#[derive(Deserialize)]
pub struct FollowData {
    #[serde(default = "default_showing_reblogs")]
    pub reblogs: bool,
    #[serde(default = "default_showing_replies")]
    pub replies: bool,
}

fn default_status_page_size() -> PageSize { PageSize::new(20) }

fn default_exclude_replies() -> bool { true }

#[derive(Deserialize)]
pub struct StatusListQueryParams {
    #[serde(default = "default_exclude_replies")]
    pub exclude_replies: bool,

    #[serde(default)]
    pub pinned: bool,

    pub max_id: Option<Uuid>,

    #[serde(default = "default_status_page_size")]
    pub limit: PageSize,
}

fn default_follow_list_page_size() -> PageSize { PageSize::new(40) }

#[derive(Deserialize)]
pub struct FollowListQueryParams {
    pub max_id: Option<i32>,

    #[serde(default = "default_follow_list_page_size")]
    pub limit: PageSize,
}

#[derive(Serialize)]
pub struct ApiSubscription {
    pub id: i32,
    pub sender: Account,
    pub sender_address: Option<String>,
    pub expires_at: DateTime<Utc>,
}

impl ApiSubscription {
    pub fn from_subscription(
        instance_url: &str,
        subscription: Subscription,
    ) -> Self {
        let sender = Account::from_profile(subscription.sender, instance_url);
        Self {
            id: subscription.id,
            sender,
            sender_address: subscription.sender_address,
            expires_at: subscription.expires_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::models::profiles::types::ProfileImage;
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
            avatar: Some(ProfileImage {
                file_name: "test".to_string(),
                media_type: None,
            }),
            ..Default::default()
        };
        let account = Account::from_profile(profile, INSTANCE_URL);

        assert_eq!(
            account.avatar.unwrap(),
            format!("{}/media/test", INSTANCE_URL),
        );
        assert!(account.source.is_none());
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
    }
}
