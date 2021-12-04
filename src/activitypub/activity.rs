use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::models::posts::types::Post;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;
use crate::utils::files::get_file_url;
use crate::utils::id::new_uuid;
use super::actor::{get_local_actor, ActorKeyError};
use super::constants::{AP_CONTEXT, AP_PUBLIC};
use super::views::{get_actor_url, get_object_url};
use super::vocabulary::*;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub name: Option<String>,

    #[serde(rename = "type")]
    pub attachment_type: String,

    pub media_type: String,
    pub url: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: String,

    #[serde(rename = "type")]
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
    pub tag: Option<Vec<Tag>>,

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
    pub to: Option<Value>,
}

fn create_activity(
    instance_url: &str,
    actor_name: &str,
    activity_type: &str,
    activity_uuid: Option<Uuid>,
    object: impl Serialize,
    recipient_id: &str,
) -> Activity {
    let actor_id = get_actor_url(
        instance_url,
        actor_name,
    );
    let activity_id = get_object_url(
        instance_url,
        &activity_uuid.unwrap_or(new_uuid()),
    );
    Activity {
        context: json!(AP_CONTEXT),
        id: activity_id,
        activity_type: activity_type.to_string(),
        actor: actor_id,
        object: serde_json::to_value(object).unwrap(),
        to: Some(json!([recipient_id])),
    }
}

pub fn create_note(
    instance_host: &str,
    instance_url: &str,
    post: &Post,
    in_reply_to: Option<&Post>,
) -> Object {
    let object_id = get_object_url(
        instance_url,
        &post.id,
    );
    let actor_id = get_actor_url(
        instance_url,
        &post.author.username,
    );
    let attachments: Vec<Attachment> = post.attachments.iter().map(|db_item| {
        let url = get_file_url(instance_url, &db_item.file_name);
        let media_type = db_item.media_type.clone().unwrap_or("".to_string());
        Attachment {
            name: None,
            attachment_type: DOCUMENT.to_string(),
            media_type,
            url,
        }
    }).collect();
    let mut recipients = vec![AP_PUBLIC.to_string()];
    let mentions: Vec<Tag> = post.mentions.iter().map(|profile| {
        let actor_id = profile.actor_id(instance_url).unwrap();
        if !profile.is_local() {
            recipients.push(actor_id.clone());
        };
        Tag {
            name: profile.actor_address(instance_host),
            tag_type: MENTION.to_string(),
            href: Some(actor_id),
        }
    }).collect();
    let in_reply_to_object_id = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            let post = in_reply_to.unwrap();
            assert_eq!(post.id, in_reply_to_id);
            if post.author.is_local() {
                Some(get_object_url(instance_url, &post.id))
            } else {
                // Replying to remote post
                let remote_actor_id = post.author.actor_id(instance_url).unwrap();
                if !recipients.contains(&remote_actor_id) {
                    recipients.push(remote_actor_id);
                };
                post.object_id.clone()
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
        attributed_to: Some(actor_id),
        in_reply_to: in_reply_to_object_id,
        content: Some(post.content.clone()),
        tag: Some(mentions),
        to: Some(json!(recipients)),
    }
}

pub fn create_activity_note(
    instance_host: &str,
    instance_url: &str,
    post: &Post,
    in_reply_to: Option<&Post>,
) -> Activity {
    let object = create_note(instance_host, instance_url, post, in_reply_to);
    let activity = create_activity(
        instance_url,
        &post.author.username,
        CREATE,
        None,
        object,
        AP_PUBLIC,
    );
    activity
}

pub fn create_activity_like(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    object_id: &str,
) -> Activity {
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: object_id.to_string(),
        object_type: NOTE.to_string(),
        ..Default::default()
    };
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        LIKE,
        None,
        object,
        AP_PUBLIC,
    );
    activity
}

pub fn create_activity_announce(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    post: &Post,
) -> Activity {
    let object_id = post.get_object_id(instance_url);
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: object_id,
        object_type: NOTE.to_string(),
        ..Default::default()
    };
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        ANNOUNCE,
        None,
        object,
        AP_PUBLIC,
    );
    activity
}

pub fn create_activity_delete_note(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    post: &Post,
) -> Activity {
    let object_id = post.get_object_id(instance_url);
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: object_id,
        object_type: TOMBSTONE.to_string(),
        ..Default::default()
    };
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        DELETE,
        None,
        object,
        AP_PUBLIC,
    );
    activity
}

pub fn create_activity_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    follow_request_id: &Uuid,
    target_actor_id: &str,
) -> Activity {
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: target_actor_id.to_owned(),
        object_type: PERSON.to_string(),
        ..Default::default()
    };
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        FOLLOW,
        Some(*follow_request_id),
        object,
        target_actor_id,
    );
    activity
}

pub fn create_activity_accept_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    follow_activity_id: &str,
    source_actor_id: &str,
) -> Activity {
    // TODO: use received activity as object
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: follow_activity_id.to_string(),
        object_type: FOLLOW.to_string(),
        ..Default::default()
    };
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        ACCEPT,
        None,
        object,
        source_actor_id,
    );
    activity
}

pub fn create_activity_undo_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    follow_request_id: &Uuid,
    target_actor_id: &str,
) -> Activity {
    // TODO: retrieve 'Follow' activity from database
    let follow_activity_id = get_object_url(
        instance_url,
        follow_request_id,
    );
    let follow_actor_id = get_actor_url(
        instance_url,
        &actor_profile.username,
    );
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: follow_activity_id,
        object_type: FOLLOW.to_string(),
        actor: Some(follow_actor_id),
        object: Some(target_actor_id.to_owned()),
        ..Default::default()
    };
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        UNDO,
        None,
        object,
        target_actor_id,
    );
    activity
}

pub fn create_activity_update_person(
    user: &User,
    instance_url: &str,
) -> Result<Activity, ActorKeyError> {
    let actor = get_local_actor(user, instance_url)?;
    let activity = create_activity(
        instance_url,
        &user.profile.username,
        UPDATE,
        None,
        actor,
        AP_PUBLIC,
    );
    Ok(activity)
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

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_HOST: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_create_note() {
        let author = DbActorProfile {
            username: "author".to_string(),
            ..Default::default()
        };
        let post = Post { author, ..Default::default() };
        let note = create_note(INSTANCE_HOST, INSTANCE_URL, &post, None);

        assert_eq!(
            note.id,
            format!("{}/objects/{}", INSTANCE_URL, post.id),
        );
        assert_eq!(note.attachment.unwrap().len(), 0);
        assert_eq!(
            note.attributed_to.unwrap(),
            format!("{}/users/{}", INSTANCE_URL, post.author.username),
        );
        assert_eq!(note.in_reply_to.is_none(), true);
        assert_eq!(note.content.unwrap(), post.content);
    }

    #[test]
    fn test_create_note_with_local_parent() {
        let parent = Post::default();
        let post = Post {
            in_reply_to_id: Some(parent.id),
            ..Default::default()
        };
        let note = create_note(INSTANCE_HOST, INSTANCE_URL, &post, Some(&parent));

        assert_eq!(
            note.in_reply_to.unwrap(),
            format!("{}/objects/{}", INSTANCE_URL, parent.id),
        );
        assert_eq!(note.to.unwrap(), json!([AP_PUBLIC]));
    }

    #[test]
    fn test_create_note_with_remote_parent() {
        let parent_author_acct = "test@test.net";
        let parent_author_actor_id = "https://test.net/user/test";
        let parent_author = DbActorProfile {
            acct: parent_author_acct.to_string(),
            actor_json: Some(json!({
                "id": parent_author_actor_id,
            })),
            ..Default::default()
        };
        let parent = Post {
            author: parent_author.clone(),
            object_id: Some("https://test.net/obj/123".to_string()),
            ..Default::default()
        };
        let post = Post {
            in_reply_to_id: Some(parent.id),
            mentions: vec![parent_author],
            ..Default::default()
        };
        let note = create_note(INSTANCE_HOST, INSTANCE_URL, &post, Some(&parent));

        assert_eq!(
            note.in_reply_to.unwrap(),
            parent.object_id.unwrap(),
        );
        let tags = note.tag.unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, parent_author_acct);
        assert_eq!(tags[0].href.as_ref().unwrap(), parent_author_actor_id);
        assert_eq!(
            note.to.unwrap(),
            json!([AP_PUBLIC, parent_author_actor_id]),
        );
    }

    #[test]
    fn test_create_activity_follow() {
        let follower = DbActorProfile {
            username: "follower".to_string(),
            ..Default::default()
        };
        let follow_request_id = new_uuid();
        let target_actor_id = "https://example.com/actor/test";
        let activity = create_activity_follow(
            INSTANCE_URL,
            &follower,
            &follow_request_id,
            target_actor_id,
        );

        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, follow_request_id),
        );
        assert_eq!(activity.activity_type, "Follow");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URL, follower.username),
        );
        assert_eq!(activity.object["id"], target_actor_id);
        assert_eq!(activity.object["type"], "Person");
        assert_eq!(activity.object["actor"], Value::Null);
        assert_eq!(activity.object["object"], Value::Null);
        assert_eq!(activity.object["content"], Value::Null);
        assert_eq!(activity.to.unwrap(), json!([target_actor_id]));
    }
}
