use std::convert::TryFrom;

use chrono::{DateTime, Duration, Utc};
use postgres_types::FromSql;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::Error as DeserializerError,
    ser::SerializeMap,
    __private::ser::FlatMapSerializer,
};
use uuid::Uuid;

use crate::activitypub::actors::types::Actor;
use crate::activitypub::identifiers::local_actor_id;
use crate::database::json_macro::{json_from_sql, json_to_sql};
use crate::errors::{ConversionError, ValidationError};
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

#[derive(PartialEq)]
pub enum PaymentType {
    Link,
    EthereumSubscription,
}

impl From<&PaymentType> for i16 {
    fn from(payment_type: &PaymentType) -> i16 {
        match payment_type {
            PaymentType::Link => 1,
            PaymentType::EthereumSubscription => 2,
        }
    }
}

impl TryFrom<i16> for PaymentType {
    type Error = ConversionError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let payment_type = match value {
            1 => Self::Link,
            2 => Self::EthereumSubscription,
            _ => return Err(ConversionError),
        };
        Ok(payment_type)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentLink {
    pub name: String,
    pub href: String,
}

#[derive(Clone, Debug)]
pub enum PaymentOption {
    Link(PaymentLink),
    EthereumSubscription,
}

impl PaymentOption {
    fn payment_type(&self) -> PaymentType {
        match self {
            Self::Link(_) => PaymentType::Link,
            Self::EthereumSubscription => PaymentType::EthereumSubscription,
        }
    }
}

// Integer tags are not supported https://github.com/serde-rs/serde/issues/745
// Workaround: https://stackoverflow.com/a/65576570
impl<'de> Deserialize<'de> for PaymentOption {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let payment_type = value.get("payment_type")
            .and_then(serde_json::Value::as_u64)
            .and_then(|val| i16::try_from(val).ok())
            .and_then(|val| PaymentType::try_from(val).ok())
            .ok_or(DeserializerError::custom("invalid payment type"))?;
        let payment_option = match payment_type {
            PaymentType::Link => {
                let link = PaymentLink::deserialize(value)
                    .map_err(DeserializerError::custom)?;
                Self::Link(link)
            },
            PaymentType::EthereumSubscription => Self::EthereumSubscription,
        };
        Ok(payment_option)
    }
}

impl Serialize for PaymentOption {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        let payment_type = self.payment_type();
        map.serialize_entry("payment_type", &i16::from(&payment_type))?;

        match self {
            Self::Link(link) => link.serialize(FlatMapSerializer(&mut map))?,
            Self::EthereumSubscription => (),
        };
        map.end()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentOptions(pub Vec<PaymentOption>);

impl PaymentOptions {
    pub fn into_inner(self) -> Vec<PaymentOption> {
        let Self(payment_options) = self;
        payment_options
    }

    pub fn is_empty(&self) -> bool {
        let Self(payment_options) = self;
        payment_options.is_empty()
    }

    /// Returns true if payment option list contains at least one option
    /// of the given type.
    pub fn any(&self, payment_type: PaymentType) -> bool {
        let Self(payment_options) = self;
        payment_options.iter()
            .any(|option| option.payment_type() == payment_type)
    }
}

json_from_sql!(PaymentOptions);
json_to_sql!(PaymentOptions);

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
    pub payment_options: PaymentOptions,
    pub extra_fields: ExtraFields,
    pub follower_count: i32,
    pub following_count: i32,
    pub post_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub actor_json: Option<Actor>,

    // auto-generated database fields
    pub actor_id: Option<String>,
}

// Profile identifiers:
// id (local profile UUID): never changes
// acct (webfinger): must never change
// actor_id of remote actor: may change if acct remains the same
// actor RSA key: can be updated at any time by the instance admin
// identity proofs: TBD (likely will do "Trust on first use" (TOFU))

impl DbActorProfile {
    pub fn is_local(&self) -> bool {
        self.actor_json.is_none()
    }

    pub fn actor_id(&self, instance_url: &str) -> String {
        // TODO: use actor_id field
        match self.actor_json {
            Some(ref actor) => actor.id.clone(),
            None => local_actor_id(instance_url, &self.username),
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

    pub fn possibly_outdated(&self) -> bool {
        if self.is_local() {
            false
        } else {
            self.updated_at < Utc::now() - Duration::days(1)
        }
    }
}

#[cfg(test)]
impl Default for DbActorProfile {
    fn default() -> Self {
        let now = Utc::now();
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
            payment_options: PaymentOptions(vec![]),
            extra_fields: ExtraFields(vec![]),
            follower_count: 0,
            following_count: 0,
            post_count: 0,
            created_at: now,
            updated_at: now,
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
    pub payment_options: Vec<PaymentOption>,
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
    pub payment_options: Vec<PaymentOption>,
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
            payment_options: profile.payment_options.into_inner(),
            extra_fields: profile.extra_fields.into_inner(),
            actor_json: profile.actor_json,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::activitypub::actors::types::Actor;
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
    fn test_payment_option_link_serialization() {
        let json_data = r#"{"payment_type":1,"name":"test","href":"https://test.com"}"#;
        let payment_option: PaymentOption = serde_json::from_str(json_data).unwrap();
        let link = match payment_option {
            PaymentOption::Link(ref link) => link,
            _ => panic!("wrong option"),
        };
        assert_eq!(link.name, "test");
        assert_eq!(link.href, "https://test.com");
        let serialized = serde_json::to_string(&payment_option).unwrap();
        assert_eq!(serialized, json_data);
    }

    #[test]
    fn test_payment_option_ethereum_subscription_serialization() {
        let json_data = r#"{"payment_type":2,"name":null,"href":null}"#;
        let payment_option: PaymentOption = serde_json::from_str(json_data).unwrap();
        assert!(matches!(
            payment_option,
            PaymentOption::EthereumSubscription,
        ));
        let serialized = serde_json::to_string(&payment_option).unwrap();
        assert_eq!(serialized, r#"{"payment_type":2}"#);
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
