use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::Instance;
use crate::models::profiles::types::ExtraField;
use crate::models::users::types::User;
use crate::utils::crypto::{deserialize_private_key, get_public_key_pem};
use crate::utils::files::get_file_url;
use super::constants::AP_CONTEXT;
use super::views::{
    get_actor_url,
    get_inbox_url,
    get_outbox_url,
    get_followers_url,
    get_following_url,
};
use super::vocabulary::{IMAGE, PERSON, PROPERTY_VALUE, SERVICE};

const W3ID_CONTEXT: &str = "https://w3id.org/security/v1";

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
pub struct Image {
    #[serde(rename = "type")]
    object_type: String,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorCapabilities {
    // Actor accepts ChatMessage objects
    accepts_chat_messages: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ActorProperty {
    name: String,
    #[serde(rename = "type")]
    object_type: String,
    value: Option<String>,
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

    pub public_key: PublicKey,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ActorCapabilities>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Image>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<Image>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<Vec<ActorProperty>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Actor {
    /// Parse 'attachment' into ExtraField vector
    pub fn extra_fields(&self) -> Vec<ExtraField> {
        let mut extra_fields = vec![];
        if let Some(properties) = &self.attachment {
            for property in properties {
                if property.object_type != PROPERTY_VALUE {
                    log::warn!(
                        "ignoring actor property of type {}",
                        property.object_type,
                    );
                    continue;
                };
                if let Some(property_value) = &property.value {
                    let field = ExtraField {
                        name: property.name.clone(),
                        value: property_value.clone(),
                        value_source: None,
                    };
                    extra_fields.push(field);
                };
            };
        };
        extra_fields
    }
}

pub struct ActorAddress {
    pub username: String,
    pub instance: String,
    pub is_local: bool,
}

impl ActorAddress {
    /// Returns acct string, as used in Mastodon
    pub fn acct(&self) -> String {
        if self.is_local {
            self.username.clone()
        } else {
            format!("{}@{}", self.username, self.instance)
        }
    }
}

pub type ActorKeyError = rsa::pkcs8::Error;

pub fn get_local_actor(
    user: &User,
    instance_url: &str,
) -> Result<Actor, ActorKeyError> {
    let username = &user.profile.username;
    let actor_id = get_actor_url(instance_url, username);
    let inbox = get_inbox_url(instance_url, username);
    let outbox = get_outbox_url(instance_url, username);
    let followers = get_followers_url(instance_url, username);
    let following = get_following_url(instance_url, username);

    let private_key = deserialize_private_key(&user.private_key)?;
    let public_key_pem = get_public_key_pem(&private_key)?;
    let public_key = PublicKey {
        id: format!("{}#main-key", actor_id),
        owner: actor_id.clone(),
        public_key_pem: public_key_pem,
    };
    let capabilities = ActorCapabilities {
        accepts_chat_messages: Some(false),
    };
    let avatar = match &user.profile.avatar_file_name {
        Some(file_name) => {
            let image = Image {
                object_type: IMAGE.to_string(),
                url: get_file_url(instance_url, file_name),
            };
            Some(image)
        },
        None => None,
    };
    let banner = match &user.profile.banner_file_name {
        Some(file_name) => {
            let image = Image {
                object_type: IMAGE.to_string(),
                url: get_file_url(instance_url, file_name),
            };
            Some(image)
        },
        None => None,
    };
    let properties = user.profile.extra_fields.clone()
        .into_inner().into_iter()
        .map(|field| {
            ActorProperty {
                object_type: PROPERTY_VALUE.to_string(),
                name: field.name,
                value: Some(field.value),
            }
        }).collect();
    let actor = Actor {
        context: Some(json!([
            AP_CONTEXT.to_string(),
            W3ID_CONTEXT.to_string(),
        ])),
        id: actor_id.clone(),
        object_type: PERSON.to_string(),
        name: user.profile.display_name.clone(),
        preferred_username: username.to_string(),
        inbox,
        outbox,
        followers: Some(followers),
        following: Some(following),
        public_key,
        capabilities: Some(capabilities),
        icon: avatar,
        image: banner,
        summary: None,
        attachment: Some(properties),
        url: Some(actor_id),
    };
    Ok(actor)
}

pub fn get_instance_actor(
    instance: &Instance,
) -> Result<Actor, ActorKeyError> {
    let actor_id = instance.actor_id();
    let actor_inbox = format!("{}/inbox", actor_id);
    let actor_outbox = format!("{}/outbox", actor_id);
    let public_key_pem = get_public_key_pem(&instance.actor_key)?;
    let public_key = PublicKey {
        id: instance.actor_key_id(),
        owner: actor_id.clone(),
        public_key_pem: public_key_pem,
    };
    let actor = Actor {
        context: Some(json!([
            AP_CONTEXT.to_string(),
            W3ID_CONTEXT.to_string(),
        ])),
        id: actor_id,
        object_type: SERVICE.to_string(),
        name: Some(instance.host()),
        preferred_username: instance.host(),
        inbox: actor_inbox,
        outbox: actor_outbox,
        followers: None,
        following: None,
        public_key,
        capabilities: None,
        icon: None,
        image: None,
        summary: None,
        attachment: None,
        url: None,
    };
    Ok(actor)
}
