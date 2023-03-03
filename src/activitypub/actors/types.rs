use std::collections::HashMap;

use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::{Error as DeserializerError},
};
use serde_json::{json, Value};

use mitra_config::Instance;
use mitra_utils::{
    crypto_rsa::{deserialize_private_key, get_public_key_pem},
    urls::get_hostname,
};

use crate::activitypub::{
    constants::{
        AP_CONTEXT,
        MASTODON_CONTEXT,
        MITRA_CONTEXT,
        SCHEMA_ORG_CONTEXT,
        W3ID_SECURITY_CONTEXT,
    },
    identifiers::{
        local_actor_id,
        local_actor_key_id,
        local_instance_actor_id,
        LocalActorCollection,
    },
    receiver::parse_property_value,
    types::deserialize_value_array,
    vocabulary::{IDENTITY_PROOF, IMAGE, LINK, PERSON, PROPERTY_VALUE, SERVICE},
};
use crate::errors::ValidationError;
use crate::media::get_file_url;
use crate::models::{
    profiles::types::{
        ExtraField,
        IdentityProof,
        PaymentOption,
    },
    users::types::User,
};
use crate::webfinger::types::ActorAddress;
use super::attachments::{
    attach_extra_field,
    attach_identity_proof,
    attach_payment_option,
    parse_extra_field,
    parse_identity_proof,
    parse_payment_option,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct PublicKey {
    id: String,
    owner: String,
    pub public_key_pem: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorImage {
    #[serde(rename = "type")]
    object_type: String,
    pub url: String,
    pub media_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorAttachment {
    pub name: String,

    #[serde(rename = "type")]
    pub object_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_algorithm: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_value: Option<String>,
}

// Some implementations use empty object instead of null
fn deserialize_image_opt<'de, D>(
    deserializer: D,
) -> Result<Option<ActorImage>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_value: Option<Value> = Option::deserialize(deserializer)?;
    let maybe_image = if let Some(value) = maybe_value {
        let is_empty_object = value.as_object()
            .map(|map| map.is_empty())
            .unwrap_or(false);
        if is_empty_object {
            None
        } else {
            let image = ActorImage::deserialize(value)
                .map_err(DeserializerError::custom)?;
            Some(image)
        }
    } else {
        None
    };
    Ok(maybe_image)
}

// Some implementations use single object instead of array
fn deserialize_attachments<'de, D>(
    deserializer: D,
) -> Result<Vec<ActorAttachment>, D::Error>
    where D: Deserializer<'de>
{
    let maybe_value: Option<Value> = Option::deserialize(deserializer)?;
    let attachments = if let Some(value) = maybe_value {
        parse_property_value(&value).map_err(DeserializerError::custom)?
    } else {
        vec![]
    };
    Ok(attachments)
}

// Clone and Debug traits are required by FromSql
#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    #[serde(rename = "@context")]
    pub context: Option<Value>,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    pub name: Option<String>,

    pub preferred_username: String,
    pub inbox: String,
    pub outbox: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub following: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribers: Option<String>,

    pub public_key: PublicKey,

    #[serde(
        default,
        deserialize_with = "deserialize_image_opt",
        skip_serializing_if = "Option::is_none",
    )]
    pub icon: Option<ActorImage>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ActorImage>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub also_known_as: Option<Value>,

    #[serde(
        default,
        deserialize_with = "deserialize_attachments",
        skip_serializing_if = "Vec::is_empty",
    )]
    pub attachment: Vec<ActorAttachment>,

    #[serde(default)]
    pub manually_approves_followers: bool,

    #[serde(
        default,
        deserialize_with = "deserialize_value_array",
        skip_serializing_if = "Vec::is_empty",
    )]
    pub tag: Vec<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Actor {
    pub fn address(
        &self,
    ) -> Result<ActorAddress, ValidationError> {
        let hostname = get_hostname(&self.id)
            .map_err(|_| ValidationError("invalid actor ID"))?;
        let actor_address = ActorAddress {
            username: self.preferred_username.clone(),
            hostname: hostname,
        };
        Ok(actor_address)
    }

    pub fn parse_attachments(&self) -> (
        Vec<IdentityProof>,
        Vec<PaymentOption>,
        Vec<ExtraField>,
    ) {
        let mut identity_proofs = vec![];
        let mut payment_options = vec![];
        let mut extra_fields = vec![];
        let log_error = |attachment: &ActorAttachment, error| {
            log::warn!(
                "ignoring actor attachment of type {}: {}",
                attachment.object_type,
                error,
            );
        };
        for attachment in self.attachment.iter() {
            match attachment.object_type.as_str() {
                IDENTITY_PROOF => {
                    match parse_identity_proof(&self.id, attachment) {
                        Ok(proof) => identity_proofs.push(proof),
                        Err(error) => log_error(attachment, error),
                    };
                },
                LINK => {
                    match parse_payment_option(attachment) {
                        Ok(option) => payment_options.push(option),
                        Err(error) => log_error(attachment, error),
                    };
                },
                PROPERTY_VALUE => {
                    match parse_extra_field(attachment) {
                        Ok(field) => extra_fields.push(field),
                        Err(error) => log_error(attachment, error),
                    };
                },
                _ => {
                    log_error(
                        attachment,
                        ValidationError("unsupported attachment type"),
                    );
                },
            };
        };
        (identity_proofs, payment_options, extra_fields)
    }
}

pub type ActorKeyError = rsa::pkcs8::Error;

fn build_actor_context() -> (
    &'static str,
    &'static str,
    HashMap<&'static str, &'static str>,
) {
    (
        AP_CONTEXT,
        W3ID_SECURITY_CONTEXT,
        HashMap::from([
            ("manuallyApprovesFollowers", "as:manuallyApprovesFollowers"),
            ("schema", SCHEMA_ORG_CONTEXT),
            ("PropertyValue", "schema:PropertyValue"),
            ("value", "schema:value"),
            ("toot", MASTODON_CONTEXT),
            ("IdentityProof", "toot:IdentityProof"),
            ("mitra", MITRA_CONTEXT),
            ("subscribers", "mitra:subscribers"),
        ]),
    )
}

pub fn get_local_actor(
    user: &User,
    instance_url: &str,
) -> Result<Actor, ActorKeyError> {
    let username = &user.profile.username;
    let actor_id = local_actor_id(instance_url, username);
    let inbox = LocalActorCollection::Inbox.of(&actor_id);
    let outbox = LocalActorCollection::Outbox.of(&actor_id);
    let followers = LocalActorCollection::Followers.of(&actor_id);
    let following = LocalActorCollection::Following.of(&actor_id);
    let subscribers = LocalActorCollection::Subscribers.of(&actor_id);

    let private_key = deserialize_private_key(&user.private_key)?;
    let public_key_pem = get_public_key_pem(&private_key)?;
    let public_key = PublicKey {
        id: local_actor_key_id(&actor_id),
        owner: actor_id.clone(),
        public_key_pem: public_key_pem,
    };
    let avatar = match &user.profile.avatar {
        Some(image) => {
            let actor_image = ActorImage {
                object_type: IMAGE.to_string(),
                url: get_file_url(instance_url, &image.file_name),
                media_type: image.media_type.clone(),
            };
            Some(actor_image)
        },
        None => None,
    };
    let banner = match &user.profile.banner {
        Some(image) => {
            let actor_image = ActorImage {
                object_type: IMAGE.to_string(),
                url: get_file_url(instance_url, &image.file_name),
                media_type: image.media_type.clone(),
            };
            Some(actor_image)
        },
        None => None,
    };
    let mut attachments = vec![];
    for proof in user.profile.identity_proofs.clone().into_inner() {
        let attachment = attach_identity_proof(proof);
        attachments.push(attachment);
    };
    for payment_option in user.profile.payment_options.clone().into_inner() {
        let attachment = attach_payment_option(
            instance_url,
            &user.id,
            payment_option,
        );
        attachments.push(attachment);
    };
    for field in user.profile.extra_fields.clone().into_inner() {
        let attachment = attach_extra_field(field);
        attachments.push(attachment);
    };
    let actor = Actor {
        context: Some(json!(build_actor_context())),
        id: actor_id.clone(),
        object_type: PERSON.to_string(),
        name: user.profile.display_name.clone(),
        preferred_username: username.to_string(),
        inbox,
        outbox,
        followers: Some(followers),
        following: Some(following),
        subscribers: Some(subscribers),
        public_key,
        icon: avatar,
        image: banner,
        summary: user.profile.bio.clone(),
        also_known_as: None,
        attachment: attachments,
        manually_approves_followers: false,
        tag: vec![],
        url: Some(actor_id),
    };
    Ok(actor)
}

pub fn get_instance_actor(
    instance: &Instance,
) -> Result<Actor, ActorKeyError> {
    let actor_id = local_instance_actor_id(&instance.url());
    let actor_inbox = LocalActorCollection::Inbox.of(&actor_id);
    let actor_outbox = LocalActorCollection::Outbox.of(&actor_id);
    let public_key_pem = get_public_key_pem(&instance.actor_key)?;
    let public_key = PublicKey {
        id: local_actor_key_id(&actor_id),
        owner: actor_id.clone(),
        public_key_pem: public_key_pem,
    };
    let actor = Actor {
        context: Some(json!(build_actor_context())),
        id: actor_id,
        object_type: SERVICE.to_string(),
        name: Some(instance.hostname()),
        preferred_username: instance.hostname(),
        inbox: actor_inbox,
        outbox: actor_outbox,
        followers: None,
        following: None,
        subscribers: None,
        public_key,
        icon: None,
        image: None,
        summary: None,
        also_known_as: None,
        attachment: vec![],
        manually_approves_followers: false,
        tag: vec![],
        url: None,
    };
    Ok(actor)
}

#[cfg(test)]
mod tests {
    use mitra_utils::crypto_rsa::{
        generate_weak_rsa_key,
        serialize_private_key,
    };
    use crate::models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_HOSTNAME: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_get_actor_address() {
        let actor = Actor {
            id: "https://test.org/users/1".to_string(),
            preferred_username: "test".to_string(),
            ..Default::default()
        };
        let actor_address = actor.address().unwrap();
        assert_eq!(actor_address.acct(INSTANCE_HOSTNAME), "test@test.org");
    }

    #[test]
    fn test_local_actor() {
        let private_key = generate_weak_rsa_key().unwrap();
        let private_key_pem = serialize_private_key(&private_key).unwrap();
        let profile = DbActorProfile {
            username: "testuser".to_string(),
            bio: Some("testbio".to_string()),
            ..Default::default()
        };
        let user = User {
            private_key: private_key_pem,
            profile,
            ..Default::default()
        };
        let actor = get_local_actor(&user, INSTANCE_URL).unwrap();
        assert_eq!(actor.id, "https://example.com/users/testuser");
        assert_eq!(actor.preferred_username, user.profile.username);
        assert_eq!(actor.inbox, "https://example.com/users/testuser/inbox");
        assert_eq!(actor.outbox, "https://example.com/users/testuser/outbox");
        assert_eq!(
            actor.followers.unwrap(),
            "https://example.com/users/testuser/followers",
        );
        assert_eq!(
            actor.following.unwrap(),
            "https://example.com/users/testuser/following",
        );
        assert_eq!(
            actor.subscribers.unwrap(),
            "https://example.com/users/testuser/subscribers",
        );
        assert_eq!(
            actor.public_key.id,
            "https://example.com/users/testuser#main-key",
        );
        assert_eq!(actor.attachment.len(), 0);
        assert_eq!(actor.summary, user.profile.bio);
    }

    #[test]
    fn test_instance_actor() {
        let instance_url = "https://example.com/";
        let instance = Instance::for_test(instance_url);
        let actor = get_instance_actor(&instance).unwrap();
        assert_eq!(actor.id, "https://example.com/actor");
        assert_eq!(actor.object_type, "Service");
        assert_eq!(actor.preferred_username, "example.com");
        assert_eq!(actor.inbox, "https://example.com/actor/inbox");
        assert_eq!(actor.outbox, "https://example.com/actor/outbox");
        assert_eq!(actor.public_key.id, "https://example.com/actor#main-key");
    }
}
