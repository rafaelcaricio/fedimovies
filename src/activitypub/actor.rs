use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::errors::ConversionError;
use crate::models::profiles::types::{DbActorProfile, ExtraField};
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
use super::vocabulary::{PERSON, IMAGE, PROPERTY_VALUE};

const W3ID_CONTEXT: &str = "https://w3id.org/security/v1";

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicKey {
    id: String,
    owner: String,
    pub public_key_pem: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    #[serde(rename = "type")]
    object_type: String,
    pub url: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorCapabilities {
    accepts_chat_messages: Option<bool>,
}

#[derive(Deserialize, Serialize)]
pub struct ActorProperty {
    name: String,
    #[serde(rename = "type")]
    object_type: String,
    value: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    #[serde(rename = "@context")]
    context: Option<Value>,

    pub id: String,

    #[serde(rename = "type")]
    object_type: String,

    pub name: String,
    pub preferred_username: String,
    pub inbox: String,
    pub outbox: String,
    pub followers: String,
    pub following: String,

    pub public_key: PublicKey,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<ActorCapabilities>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Image>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<Image>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,

    pub attachment: Option<Vec<ActorProperty>>,
}

impl Actor {
    /// Parse 'attachment' into ExtraField vector
    pub fn extra_fields(&self) -> Vec<ExtraField> {
        match &self.attachment {
            Some(properties) => {
                properties.iter()
                    .map(|prop| ExtraField {
                        name: prop.name.clone(),
                        value: prop.value.clone(),
                        value_source: None,
                    })
                    .collect()
            },
            None => vec![],
        }
    }
}

impl DbActorProfile {
    pub fn remote_actor(&self) -> Result<Option<Actor>, ConversionError> {
        let actor = match self.actor_json {
            Some(ref value) => {
                let actor: Actor = serde_json::from_value(value.clone())
                    .map_err(|_| ConversionError)?;
                Some(actor)
            },
            None => None,
        };
        Ok(actor)
    }
}

pub type ActorKeyError = rsa::pkcs8::Error;

pub fn get_local_actor(
    user: &User,
    instance_url: &str,
) -> Result<Actor, ActorKeyError> {
    let username = &user.profile.username;
    let id = get_actor_url(instance_url, username);
    let inbox = get_inbox_url(instance_url, username);
    let outbox = get_outbox_url(instance_url, username);
    let followers = get_followers_url(instance_url, username);
    let following = get_following_url(instance_url, username);

    let private_key = deserialize_private_key(&user.private_key)?;
    let public_key_pem = get_public_key_pem(&private_key)?;
    let public_key = PublicKey {
        id: format!("{}#main-key", id),
        owner: id.clone(),
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
        .unpack().into_iter()
        .map(|field| {
            ActorProperty {
                object_type: PROPERTY_VALUE.to_string(),
                name: field.name,
                value: field.value,
            }
        }).collect();
    let actor = Actor {
        context: Some(json!([
            AP_CONTEXT.to_string(),
            W3ID_CONTEXT.to_string(),
        ])),
        id,
        object_type: PERSON.to_string(),
        name: username.to_string(),
        preferred_username: username.to_string(),
        inbox,
        outbox,
        followers,
        following,
        public_key,
        capabilities: Some(capabilities),
        icon: avatar,
        image: banner,
        summary: None,
        attachment: Some(properties),
    };
    Ok(actor)
}
