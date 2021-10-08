use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::Config;
use crate::models::posts::types::Post;
use crate::models::profiles::types::DbActorProfile;
use crate::utils::files::get_file_url;
use super::constants::{AP_CONTEXT, AP_PUBLIC};
use super::views::{get_actor_url, get_object_url};
use super::vocabulary::*;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: String,

    #[serde(rename = "type")]
    pub attachment_type: String,

    pub media_type: String,
    pub url: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Object {
    #[serde(rename = "@context")]
    pub context: Option<Value>,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<Vec<Attachment>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributed_to: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Value>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    #[serde(rename = "@context")]
    pub context: Value,

    pub id: String,
    
    #[serde(rename = "type")]
    pub activity_type: String,

    pub actor: String,
    pub object: Value,
}

fn create_activity(
    instance_url: &str,
    actor_name: &str,
    activity_type: &str,
    activity_uuid: Option<Uuid>,
    object: Value,
) -> Activity {
    let actor_id = get_actor_url(
        instance_url,
        &actor_name,
    );
    let activity_id = get_object_url(
        instance_url,
        &activity_uuid.unwrap_or(Uuid::new_v4()),
    );
    let activity = Activity {
        context: json!(AP_CONTEXT),
        id: activity_id,
        activity_type: activity_type.to_string(),
        actor: actor_id,
        object: object,
    };
    activity
}

pub fn create_note(
    config: &Config,
    post: &Post,
    in_reply_to: Option<&Post>,
) -> Object {
    let object_id = get_object_url(
        &config.instance_url(),
        &post.id,
    );
    let actor_id = get_actor_url(
        &config.instance_url(),
        &post.author.username,
    );
    let attachments: Vec<Attachment> = post.attachments.iter().map(|db_item| {
        let url = get_file_url(&config.instance_url(), &db_item.file_name);
        let media_type = db_item.media_type.clone().unwrap_or("".to_string());
        Attachment {
            name: "".to_string(),
            attachment_type: DOCUMENT.to_string(),
            media_type,
            url,
        }
    }).collect();
    let in_reply_to_object_id = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            let post = in_reply_to.unwrap();
            assert_eq!(post.id, in_reply_to_id);
            match post.author.actor_json {
                Some(_) => None, // TODO: store object ID for remote posts
                None => Some(get_object_url(&config.instance_url(), &post.id)),
            }
        },
        None => None,
    };
    Object {
        context: Some(json!(AP_CONTEXT)),
        id: object_id,
        object_type: NOTE.to_string(),
        actor: None,
        attachment: Some(attachments),
        object: None,
        published: Some(post.created_at),
        attributed_to: Some(actor_id.clone()),
        in_reply_to: in_reply_to_object_id,
        content: Some(post.content.clone()),
        to: Some(json!(AP_PUBLIC)),
    }
}

pub fn create_activity_note(
    config: &Config,
    post: &Post,
    in_reply_to: Option<&Post>,
) -> Activity {
    let object = create_note(config, post, in_reply_to);
    let activity = create_activity(
        &config.instance_url(),
        &post.author.username,
        CREATE,
        None,
        serde_json::to_value(object).unwrap(),
    );
    activity
}

pub fn create_activity_follow(
    config: &Config,
    actor_profile: &DbActorProfile,
    follow_request_id: &Uuid,
    target_id: &str,
) -> Activity {
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: target_id.to_owned(),
        object_type: PERSON.to_string(),
        actor: None,
        attachment: None,
        object: None,
        published: None,
        attributed_to: None,
        in_reply_to: None,
        content: None,
        to: None,
    };
    let activity = create_activity(
        &config.instance_url(),
        &actor_profile.username,
        FOLLOW,
        Some(*follow_request_id),
        serde_json::to_value(object).unwrap(),
    );
    activity
}

pub fn create_activity_accept_follow(
    config: &Config,
    actor_profile: &DbActorProfile,
    follow_activity_id: &str,
) -> Activity {
    // TODO: use received activity as object
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: follow_activity_id.to_string(),
        object_type: FOLLOW.to_string(),
        actor: None,
        attachment: None,
        object: None,
        published: None,
        attributed_to: None,
        in_reply_to: None,
        content: None,
        to: None,
    };
    let activity = create_activity(
        &config.instance_url(),
        &actor_profile.username,
        ACCEPT,
        None,
        serde_json::to_value(object).unwrap(),
    );
    activity
}

pub fn create_activity_undo_follow(
    config: &Config,
    actor_profile: &DbActorProfile,
    follow_request_id: &Uuid,
    target_id: &str,
) -> Activity {
    // TODO: retrieve 'Follow' activity from database
    let follow_activity_id = get_object_url(
        &config.instance_url(),
        follow_request_id,
    );
    let follow_actor_id = get_actor_url(
        &config.instance_url(),
        &actor_profile.username,
    );
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: follow_activity_id,
        object_type: FOLLOW.to_string(),
        actor: Some(follow_actor_id),
        attachment: None,
        object: Some(target_id.to_owned()),
        published: None,
        attributed_to: None,
        in_reply_to: None,
        content: None,
        to: None,
    };
    let activity = create_activity(
        &config.instance_url(),
        &actor_profile.username,
        UNDO,
        None,
        serde_json::to_value(object).unwrap(),
    );
    activity
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedCollection {
    #[serde(rename = "@context")]
    pub context: Value,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,
}

impl OrderedCollection {
    pub fn new(collection_url: String) -> Self {
        Self {
            context: json!(AP_CONTEXT),
            id: collection_url,
            object_type: "OrderedCollection".to_string(),
        }
    }
}
