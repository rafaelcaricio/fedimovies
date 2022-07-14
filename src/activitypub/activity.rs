use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::profiles::types::DbActorProfile;
use super::constants::{AP_CONTEXT, AP_PUBLIC};
use super::views::{
    get_actor_url,
    get_followers_url,
    get_object_url,
};
use super::vocabulary::*;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: Option<String>,

    #[serde(rename = "type")]
    pub attachment_type: String,

    pub media_type: Option<String>,
    pub url: Option<String>,
}

fn default_tag_type() -> String { HASHTAG.to_string() }

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: Option<String>,

    #[serde(rename = "type", default = "default_tag_type")]
    pub tag_type: String,

    pub href: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Object {
    #[serde(rename = "@context")]
    pub context: Option<Value>,

    pub id: String,

    #[serde(rename = "type")]
    pub object_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub former_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<DateTime<Utc>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributed_to: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<Vec<Tag>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<DateTime<Utc>>,
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
    pub to: Option<Value>,
    pub cc: Option<Value>,
}

pub fn create_activity(
    instance_url: &str,
    actor_name: &str,
    activity_type: &str,
    activity_id: String,
    object: impl Serialize,
    primary_audience: Vec<String>,
    secondary_audience: Vec<String>,
) -> Activity {
    let actor_id = get_actor_url(
        instance_url,
        actor_name,
    );
    Activity {
        context: json!(AP_CONTEXT),
        id: activity_id,
        activity_type: activity_type.to_string(),
        actor: actor_id,
        object: serde_json::to_value(object).unwrap(),
        to: Some(json!(primary_audience)),
        cc: Some(json!(secondary_audience)),
    }
}

pub fn create_activity_undo_announce(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    repost_id: &Uuid,
    recipient_id: &str,
) -> Activity {
    let object_id = get_object_url(
        instance_url,
        repost_id,
    );
    let activity_id = format!("{}/undo", object_id);
    let primary_audience = vec![
        AP_PUBLIC.to_string(),
        recipient_id.to_string(),
    ];
    create_activity(
        instance_url,
        &actor_profile.username,
        UNDO,
        activity_id,
        object_id,
        primary_audience,
        vec![get_followers_url(instance_url, &actor_profile.username)],
    )
}

#[cfg(test)]
mod tests {
    use crate::utils::id::new_uuid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_create_activity_undo_announce() {
        let announcer = DbActorProfile::default();
        let post_author_id = "https://example.com/users/test";
        let repost_id = new_uuid();
        let activity = create_activity_undo_announce(
            INSTANCE_URL,
            &announcer,
            &repost_id,
            post_author_id,
        );
        assert_eq!(
            activity.id,
            format!("{}/objects/{}/undo", INSTANCE_URL, repost_id),
        );
        assert_eq!(
            activity.object,
            format!("{}/objects/{}", INSTANCE_URL, repost_id),
        );
        assert_eq!(activity.to.unwrap(), json!([AP_PUBLIC, post_author_id]));
    }
}
