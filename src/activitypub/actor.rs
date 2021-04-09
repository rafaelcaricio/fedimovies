use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::Config;
use crate::errors::HttpError;
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
use super::vocabulary::{PERSON, IMAGE};

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
}

pub fn get_actor_object(
    config: &Config,
    user: &User,
) -> Result<Actor, HttpError> {
    let username = &user.profile.username;
    let id = get_actor_url(&config.instance_url(), &username);
    let inbox = get_inbox_url(&config.instance_url(), &username);
    let outbox = get_outbox_url(&config.instance_url(), &username);
    let followers = get_followers_url(&config.instance_url(), &username);
    let following = get_following_url(&config.instance_url(), &username);

    let private_key = deserialize_private_key(&user.private_key)
        .map_err(|_| HttpError::InternalError)?;
    let public_key_pem = get_public_key_pem(&private_key)
        .map_err(|_| HttpError::InternalError)?;
    let public_key = PublicKey {
        id: format!("{}#main-key", id),
        owner: id.clone(),
        public_key_pem: public_key_pem,
    };
    let avatar = match &user.profile.avatar_file_name {
        Some(file_name) => {
            let image = Image {
                object_type: IMAGE.to_string(),
                url: get_file_url(&config.instance_url(), file_name),
            };
            Some(image)
        },
        None => None,
    };
    let banner = match &user.profile.banner_file_name {
        Some(file_name) => {
            let image = Image {
                object_type: IMAGE.to_string(),
                url: get_file_url(&config.instance_url(), file_name),
            };
            Some(image)
        },
        None => None,
    };
    let capabilities = ActorCapabilities {
        accepts_chat_messages: Some(false),
    };
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
    };
    Ok(actor)
}
