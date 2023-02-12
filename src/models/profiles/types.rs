use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use postgres_types::FromSql;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::Error as DeserializerError,
    ser::SerializeMap,
    __private::ser::FlatMapSerializer,
};
use uuid::Uuid;

use crate::activitypub::{
    actors::types::Actor,
    identifiers::local_actor_id,
};
use crate::database::{
    json_macro::{json_from_sql, json_to_sql},
    DatabaseTypeError,
};
use crate::errors::{ConversionError, ValidationError};
use crate::identity::{
    did::Did,
    signatures::{PROOF_TYPE_ID_EIP191, PROOF_TYPE_ID_MINISIGN},
};
use crate::utils::caip2::ChainId;
use crate::webfinger::types::ActorAddress;
use super::validators::{
    validate_username,
    validate_display_name,
    clean_bio,
    clean_extra_fields,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProfileImage {
    pub file_name: String,
    pub file_size: Option<usize>,
    pub media_type: Option<String>,
}

impl ProfileImage {
    pub fn new(
        file_name: String,
        file_size: usize,
        media_type: Option<String>,
    ) -> Self {
        Self {
            file_name,
            file_size: Some(file_size),
            media_type,
        }
    }
}

json_from_sql!(ProfileImage);
json_to_sql!(ProfileImage);

#[derive(Clone, Debug)]
pub enum ProofType {
    LegacyEip191IdentityProof,
    LegacyMinisignIdentityProof,
}

impl FromStr for ProofType {
    type Err = ConversionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let proof_type = match value {
            PROOF_TYPE_ID_EIP191 => Self::LegacyEip191IdentityProof,
            PROOF_TYPE_ID_MINISIGN => Self::LegacyMinisignIdentityProof,
            _ => return Err(ConversionError),
        };
        Ok(proof_type)
    }
}

impl fmt::Display for ProofType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let proof_type_str = match self {
            Self::LegacyEip191IdentityProof => PROOF_TYPE_ID_EIP191,
            Self::LegacyMinisignIdentityProof => PROOF_TYPE_ID_MINISIGN,
        };
        write!(formatter, "{}", proof_type_str)
    }
}

impl<'de> Deserialize<'de> for ProofType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        String::deserialize(deserializer)?
            .parse().map_err(DeserializerError::custom)
    }
}

impl Serialize for ProofType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdentityProof {
    pub issuer: Did,
    pub proof_type: ProofType,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IdentityProofs(pub Vec<IdentityProof>);

impl IdentityProofs {
    pub fn inner(&self) -> &[IdentityProof] {
        let Self(identity_proofs) = self;
        identity_proofs
    }

    pub fn into_inner(self) -> Vec<IdentityProof> {
        let Self(identity_proofs) = self;
        identity_proofs
    }

    /// Returns true if identity proof list contains at least one proof
    /// created by a given DID.
    pub fn any(&self, issuer: &Did) -> bool {
        let Self(identity_proofs) = self;
        identity_proofs.iter().any(|proof| proof.issuer == *issuer)
    }
}

json_from_sql!(IdentityProofs);
json_to_sql!(IdentityProofs);

#[derive(PartialEq)]
pub enum PaymentType {
    Link,
    EthereumSubscription,
    MoneroSubscription,
}

impl From<&PaymentType> for i16 {
    fn from(payment_type: &PaymentType) -> i16 {
        match payment_type {
            PaymentType::Link => 1,
            PaymentType::EthereumSubscription => 2,
            PaymentType::MoneroSubscription => 3,
        }
    }
}

impl TryFrom<i16> for PaymentType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let payment_type = match value {
            1 => Self::Link,
            2 => Self::EthereumSubscription,
            3 => Self::MoneroSubscription,
            _ => return Err(DatabaseTypeError),
        };
        Ok(payment_type)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentLink {
    pub name: String,
    pub href: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EthereumSubscription {
    chain_id: ChainId,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MoneroSubscription {
    pub chain_id: ChainId,
    pub price: u64, // piconeros per second
    pub payout_address: String,
}

#[derive(Clone, Debug)]
pub enum PaymentOption {
    Link(PaymentLink),
    EthereumSubscription(EthereumSubscription),
    MoneroSubscription(MoneroSubscription),
}

impl PaymentOption {
    pub fn ethereum_subscription(chain_id: ChainId) -> Self {
        Self::EthereumSubscription(EthereumSubscription { chain_id })
    }

    fn payment_type(&self) -> PaymentType {
        match self {
            Self::Link(_) => PaymentType::Link,
            Self::EthereumSubscription(_) => PaymentType::EthereumSubscription,
            Self::MoneroSubscription(_) => PaymentType::MoneroSubscription,
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
            PaymentType::EthereumSubscription => {
                let payment_info = EthereumSubscription::deserialize(value)
                    .map_err(DeserializerError::custom)?;
                Self::EthereumSubscription(payment_info)
            },
            PaymentType::MoneroSubscription => {
                let payment_info = MoneroSubscription::deserialize(value)
                    .map_err(DeserializerError::custom)?;
                Self::MoneroSubscription(payment_info)
            },
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
            Self::EthereumSubscription(payment_info) => {
                payment_info.serialize(FlatMapSerializer(&mut map))?
            },
            Self::MoneroSubscription(payment_info) => {
                payment_info.serialize(FlatMapSerializer(&mut map))?
            },
        };
        map.end()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PaymentOptions(pub Vec<PaymentOption>);

impl PaymentOptions {
    pub fn inner(&self) -> &[PaymentOption] {
        let Self(payment_options) = self;
        payment_options
    }

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
    pub hostname: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>, // html
    pub bio_source: Option<String>, // plaintext or markdown
    pub avatar: Option<ProfileImage>,
    pub banner: Option<ProfileImage>,
    pub identity_proofs: IdentityProofs,
    pub payment_options: PaymentOptions,
    pub extra_fields: ExtraFields,
    pub follower_count: i32,
    pub following_count: i32,
    pub subscriber_count: i32,
    pub post_count: i32,
    pub actor_json: Option<Actor>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub unreachable_since: Option<DateTime<Utc>>,

    // auto-generated database fields
    pub acct: String,
    pub actor_id: Option<String>,
}

// Profile identifiers:
// id (local profile UUID): never changes
// acct (webfinger): must never change
// actor_id of remote actor: may change if acct remains the same
// actor RSA key: can be updated at any time by the instance admin
// identity proofs: TBD (likely will do "Trust on first use" (TOFU))

impl DbActorProfile {
    pub fn check_remote(&self) -> Result<(), DatabaseTypeError> {
        // Consistency checks
        if self.hostname.is_none() || self.actor_json.is_none() {
            return Err(DatabaseTypeError);
        };
        Ok(())
    }

    pub fn is_local(&self) -> bool {
        self.actor_json.is_none()
    }

    pub fn actor_id(&self, instance_url: &str) -> String {
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

    pub fn actor_address(&self, local_hostname: &str) -> ActorAddress {
        assert_eq!(self.hostname.is_none(), self.is_local());
        ActorAddress {
            username: self.username.clone(),
            hostname: self.hostname.as_deref()
                .unwrap_or(local_hostname)
                .to_string(),
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
            hostname: None,
            acct: "".to_string(),
            display_name: None,
            bio: None,
            bio_source: None,
            avatar: None,
            banner: None,
            identity_proofs: IdentityProofs(vec![]),
            payment_options: PaymentOptions(vec![]),
            extra_fields: ExtraFields(vec![]),
            follower_count: 0,
            following_count: 0,
            subscriber_count: 0,
            post_count: 0,
            actor_json: None,
            actor_id: None,
            created_at: now,
            updated_at: now,
            unreachable_since: None,
        }
    }
}

#[cfg_attr(test, derive(Default))]
pub struct ProfileCreateData {
    pub username: String,
    pub hostname: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar: Option<ProfileImage>,
    pub banner: Option<ProfileImage>,
    pub identity_proofs: Vec<IdentityProof>,
    pub payment_options: Vec<PaymentOption>,
    pub extra_fields: Vec<ExtraField>,
    pub actor_json: Option<Actor>,
}

impl ProfileCreateData {
    pub fn clean(&mut self) -> Result<(), ValidationError> {
        validate_username(&self.username)?;
        if self.hostname.is_some() != self.actor_json.is_some() {
            return Err(ValidationError("hostname and actor_json field mismatch"));
        };
        if let Some(display_name) = &self.display_name {
            validate_display_name(display_name)?;
        };
        let is_remote = self.actor_json.is_some();
        if let Some(bio) = &self.bio {
            let cleaned_bio = clean_bio(bio, is_remote)?;
            self.bio = Some(cleaned_bio);
        };
        self.extra_fields = clean_extra_fields(&self.extra_fields, is_remote)?;
        Ok(())
    }
}

pub struct ProfileUpdateData {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub bio_source: Option<String>,
    pub avatar: Option<ProfileImage>,
    pub banner: Option<ProfileImage>,
    pub identity_proofs: Vec<IdentityProof>,
    pub payment_options: Vec<PaymentOption>,
    pub extra_fields: Vec<ExtraField>,
    pub actor_json: Option<Actor>,
}

impl ProfileUpdateData {
    /// Adds new identity proof
    /// or replaces the existing one if it has the same issuer.
    pub fn add_identity_proof(&mut self, proof: IdentityProof) -> () {
        self.identity_proofs.retain(|item| item.issuer != proof.issuer);
        self.identity_proofs.push(proof);
    }

    /// Adds new payment option
    /// or replaces the existing one if it has the same type.
    pub fn add_payment_option(&mut self, option: PaymentOption) -> () {
        self.payment_options.retain(|item| {
            item.payment_type() != option.payment_type()
        });
        self.payment_options.push(option);
    }

    pub fn clean(&mut self) -> Result<(), ValidationError> {
        if let Some(display_name) = &self.display_name {
            validate_display_name(display_name)?;
        };
        let is_remote = self.actor_json.is_some();
        // Validate and clean bio
        if let Some(bio) = &self.bio {
            let cleaned_bio = clean_bio(bio, is_remote)?;
            self.bio = Some(cleaned_bio);
        };
        self.extra_fields = clean_extra_fields(&self.extra_fields, is_remote)?;
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
            avatar: profile.avatar,
            banner: profile.banner,
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

    const INSTANCE_HOSTNAME: &str = "example.com";

    #[test]
    fn test_identity_proof_serialization() {
        let json_data = r#"{"issuer":"did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a","proof_type":"ethereum-eip191-00","value":"dbfe"}"#;
        let proof: IdentityProof = serde_json::from_str(json_data).unwrap();
        let did_pkh = match proof.issuer {
            Did::Pkh(ref did_pkh) => did_pkh,
            _ => panic!("unexpected did method"),
        };
        assert_eq!(did_pkh.address, "0xb9c5714089478a327f09197987f16f9e5d936e8a");
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
        let json_data = r#"{"payment_type":2,"chain_id":"eip155:1","name":null}"#;
        let payment_option: PaymentOption = serde_json::from_str(json_data).unwrap();
        let payment_info = match payment_option {
            PaymentOption::EthereumSubscription(ref payment_info) => payment_info,
            _ => panic!("wrong option"),
        };
        assert_eq!(payment_info.chain_id, ChainId::ethereum_mainnet());
        let serialized = serde_json::to_string(&payment_option).unwrap();
        assert_eq!(serialized, r#"{"payment_type":2,"chain_id":"eip155:1"}"#);
    }

    #[test]
    fn test_local_actor_address() {
        let local_profile = DbActorProfile {
            username: "user".to_string(),
            hostname: None,
            acct: "user".to_string(),
            actor_json: None,
            ..Default::default()
        };
        assert_eq!(
            local_profile.actor_address(INSTANCE_HOSTNAME).to_string(),
            "user@example.com",
        );
    }

    #[test]
    fn test_remote_actor_address() {
        let remote_profile = DbActorProfile {
            username: "test".to_string(),
            hostname: Some("remote.com".to_string()),
            acct: "test@remote.com".to_string(),
            actor_json: Some(Actor {
                id: "https://test".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            remote_profile.actor_address(INSTANCE_HOSTNAME).to_string(),
            remote_profile.acct,
        );
    }

    #[test]
    fn test_clean_profile_create_data() {
        let mut profile_data = ProfileCreateData {
            username: "test".to_string(),
            hostname: Some("example.org".to_string()),
            display_name: Some("Test Test".to_string()),
            actor_json: Some(Actor {
                id: "https://example.org/test".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = profile_data.clean();
        assert_eq!(result.is_ok(), true);
    }
}
