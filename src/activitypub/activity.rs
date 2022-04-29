use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::frontend::get_tag_page_url;
use crate::models::posts::types::{Post, Visibility};
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;
use crate::utils::files::get_file_url;
use crate::utils::id::new_uuid;
use super::actor::{get_local_actor, ActorKeyError};
use super::constants::{AP_CONTEXT, AP_PUBLIC};
use super::views::{
    get_actor_url,
    get_followers_url,
    get_subscribers_url,
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    #[serde(rename = "@context")]
    context: String,

    id: String,

    #[serde(rename = "type")]
    object_type: String,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachment: Vec<Attachment>,

    attributed_to: String,

    content: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    in_reply_to: Option<String>,

    published: DateTime<Utc>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    tag: Vec<Tag>,

    to: Vec<String>,
    cc: Vec<String>,
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

fn create_activity(
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

pub fn create_note(
    instance_host: &str,
    instance_url: &str,
    post: &Post,
) -> Note {
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
        let media_type = db_item.media_type.clone();
        Attachment {
            name: None,
            attachment_type: DOCUMENT.to_string(),
            media_type,
            url: Some(url),
        }
    }).collect();
    let mut primary_audience = vec![];
    let mut secondary_audience = vec![];
    let followers_collection_url =
        get_followers_url(instance_url, &post.author.username);
    let subscribers_collection_url =
        get_subscribers_url(instance_url, &post.author.username);
    match post.visibility {
        Visibility::Public => {
            primary_audience.push(AP_PUBLIC.to_string());
            secondary_audience.push(followers_collection_url);
        },
        Visibility::Followers => {
            primary_audience.push(followers_collection_url);
        },
        Visibility::Subscribers => {
            primary_audience.push(subscribers_collection_url);
        },
        Visibility::Direct => (),
    };
    let mut tags = vec![];
    for profile in &post.mentions {
        let tag_name = format!("@{}", profile.actor_address(instance_host));
        let actor_id = profile.actor_id(instance_url);
        primary_audience.push(actor_id.clone());
        let tag = Tag {
            name: Some(tag_name),
            tag_type: MENTION.to_string(),
            href: Some(actor_id),
        };
        tags.push(tag);
    };
    for tag_name in &post.tags {
        let tag_page_url = get_tag_page_url(instance_url, tag_name);
        let tag = Tag {
            name: Some(format!("#{}", tag_name)),
            tag_type: HASHTAG.to_string(),
            href: Some(tag_page_url),
        };
        tags.push(tag);
    };
    let in_reply_to_object_id = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            let in_reply_to = post.in_reply_to.as_ref().unwrap();
            assert_eq!(in_reply_to.id, in_reply_to_id);
            let in_reply_to_actor_id = in_reply_to.author.actor_id(instance_url);
            if !primary_audience.contains(&in_reply_to_actor_id) {
                primary_audience.push(in_reply_to_actor_id);
            };
            Some(in_reply_to.get_object_id(instance_url))
        },
        None => None,
    };
    Note {
        context: AP_CONTEXT.to_string(),
        id: object_id,
        object_type: NOTE.to_string(),
        attachment: attachments,
        published: post.created_at,
        attributed_to: actor_id,
        in_reply_to: in_reply_to_object_id,
        content: post.content.clone(),
        tag: tags,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub fn create_activity_note(
    instance_host: &str,
    instance_url: &str,
    post: &Post,
) -> Activity {
    let object = create_note(instance_host, instance_url, post);
    let primary_audience = object.to.clone();
    let secondary_audience = object.cc.clone();
    let activity_id = get_object_url(instance_url, &post.id) + "/create";
    let activity = create_activity(
        instance_url,
        &post.author.username,
        CREATE,
        activity_id,
        object,
        primary_audience,
        secondary_audience,
    );
    activity
}

pub fn create_activity_like(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    note_id: &str,
    reaction_id: &Uuid,
    recipient_id: &str,
) -> Activity {
    let activity_id = get_object_url(instance_url, reaction_id);
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        LIKE,
        activity_id,
        note_id,
        vec![AP_PUBLIC.to_string(), recipient_id.to_string()],
        vec![],
    );
    activity
}

pub fn create_activity_undo_like(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    reaction_id: &Uuid,
    recipient_id: &str,
) -> Activity {
    let object_id = get_object_url(
        instance_url,
        reaction_id,
    );
    let activity_id = get_object_url(instance_url, reaction_id) + "/undo";
    create_activity(
        instance_url,
        &actor_profile.username,
        UNDO,
        activity_id,
        object_id,
        vec![AP_PUBLIC.to_string(), recipient_id.to_string()],
        vec![],
    )
}

pub fn create_activity_announce(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    post: &Post,
    repost_id: &Uuid,
) -> Activity {
    let object_id = post.get_object_id(instance_url);
    let activity_id = get_object_url(instance_url, repost_id);
    let recipient_id = post.author.actor_id(instance_url);
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        ANNOUNCE,
        activity_id,
        object_id,
        vec![AP_PUBLIC.to_string(), recipient_id],
        vec![get_followers_url(instance_url, &actor_profile.username)],
    );
    activity
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
    let activity_id = get_object_url(instance_url, repost_id) + "/undo";
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

pub fn create_activity_delete_note(
    instance_url: &str,
    post: &Post,
) -> Activity {
    let object_id = post.get_object_id(instance_url);
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: object_id,
        object_type: TOMBSTONE.to_string(),
        former_type: Some(NOTE.to_string()),
        ..Default::default()
    };
    let activity_id = get_object_url(instance_url, &post.id) + "/delete";
    let activity = create_activity(
        instance_url,
        &post.author.username,
        DELETE,
        activity_id,
        object,
        vec![AP_PUBLIC.to_string()],
        vec![],
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
    let activity_id = get_object_url(instance_url, follow_request_id);
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        FOLLOW,
        activity_id,
        object,
        vec![target_actor_id.to_string()],
        vec![],
    );
    activity
}

pub fn create_activity_accept_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    follow_activity_id: &str,
    source_actor_id: &str,
) -> Activity {
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: follow_activity_id.to_string(),
        object_type: FOLLOW.to_string(),
        ..Default::default()
    };
    let activity_id = follow_activity_id.to_string() + "/accept";
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        ACCEPT,
        activity_id,
        object,
        vec![source_actor_id.to_string()],
        vec![],
    );
    activity
}

pub fn create_activity_undo_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    follow_request_id: &Uuid,
    target_actor_id: &str,
) -> Activity {
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
        id: follow_activity_id.clone(),
        object_type: FOLLOW.to_string(),
        actor: Some(follow_actor_id),
        object: Some(target_actor_id.to_owned()),
        ..Default::default()
    };
    let activity_id = follow_activity_id + "/undo";
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        UNDO,
        activity_id,
        object,
        vec![target_actor_id.to_string()],
        vec![],
    );
    activity
}

pub fn create_activity_update_person(
    user: &User,
    instance_url: &str,
) -> Result<Activity, ActorKeyError> {
    let actor = get_local_actor(user, instance_url)?;
    // Update(Person) is idempotent so its ID can be random
    let activity_id = get_object_url(instance_url, &new_uuid());
    let activity = create_activity(
        instance_url,
        &user.profile.username,
        UPDATE,
        activity_id,
        actor,
        vec![
            AP_PUBLIC.to_string(),
            get_followers_url(instance_url, &user.profile.username),
        ],
        vec![],
    );
    Ok(activity)
}

#[cfg(test)]
mod tests {
    use crate::activitypub::actor::Actor;
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
        let note = create_note(INSTANCE_HOST, INSTANCE_URL, &post);

        assert_eq!(
            note.id,
            format!("{}/objects/{}", INSTANCE_URL, post.id),
        );
        assert_eq!(note.attachment.len(), 0);
        assert_eq!(
            note.attributed_to,
            format!("{}/users/{}", INSTANCE_URL, post.author.username),
        );
        assert_eq!(note.in_reply_to.is_none(), true);
        assert_eq!(note.content, post.content);
        assert_eq!(note.to, vec![AP_PUBLIC]);
        assert_eq!(note.cc, vec![
            get_followers_url(INSTANCE_URL, "author"),
        ]);
    }

    #[test]
    fn test_create_note_followers_only() {
        let post = Post {
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let note = create_note(INSTANCE_HOST, INSTANCE_URL, &post);

        assert_eq!(note.to, vec![
            get_followers_url(INSTANCE_URL, &post.author.username),
        ]);
        assert_eq!(note.cc.is_empty(), true);
    }

    #[test]
    fn test_create_note_with_local_parent() {
        let parent = Post::default();
        let post = Post {
            in_reply_to_id: Some(parent.id),
            in_reply_to: Some(Box::new(parent.clone())),
            ..Default::default()
        };
        let note = create_note(INSTANCE_HOST, INSTANCE_URL, &post);

        assert_eq!(
            note.in_reply_to.unwrap(),
            format!("{}/objects/{}", INSTANCE_URL, parent.id),
        );
        assert_eq!(note.to, vec![
            AP_PUBLIC.to_string(),
            get_actor_url(INSTANCE_URL, &parent.author.username),
        ]);
    }

    #[test]
    fn test_create_note_with_remote_parent() {
        let parent_author_acct = "test@test.net";
        let parent_author_actor_id = "https://test.net/user/test";
        let parent_author_actor_url = "https://test.net/@test";
        let parent_author = DbActorProfile {
            acct: parent_author_acct.to_string(),
            actor_json: Some(Actor {
                id: parent_author_actor_id.to_string(),
                url: Some(parent_author_actor_url.to_string()),
                ..Default::default()
            }),
            actor_id: Some(parent_author_actor_id.to_string()),
            ..Default::default()
        };
        let parent = Post {
            author: parent_author.clone(),
            object_id: Some("https://test.net/obj/123".to_string()),
            ..Default::default()
        };
        let post = Post {
            in_reply_to_id: Some(parent.id),
            in_reply_to: Some(Box::new(parent.clone())),
            mentions: vec![parent_author],
            ..Default::default()
        };
        let note = create_note(INSTANCE_HOST, INSTANCE_URL, &post);

        assert_eq!(
            note.in_reply_to.unwrap(),
            parent.object_id.unwrap(),
        );
        let tags = note.tag;
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name.as_deref().unwrap(), format!("@{}", parent_author_acct));
        assert_eq!(tags[0].href.as_ref().unwrap(), parent_author_actor_id);
        assert_eq!(note.to, vec![AP_PUBLIC, parent_author_actor_id]);
    }

    #[test]
    fn test_create_activity_create_note() {
        let author_username = "author";
        let author = DbActorProfile {
            username: author_username.to_string(),
            ..Default::default()
        };
        let post = Post { author, ..Default::default() };
        let activity = create_activity_note(INSTANCE_HOST, INSTANCE_URL, &post);

        assert_eq!(
            activity.id,
            format!("{}/objects/{}/create", INSTANCE_URL, post.id),
        );
        assert_eq!(activity.activity_type, CREATE);
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URL, author_username),
        );
        assert_eq!(activity.to.unwrap(), json!([AP_PUBLIC]));
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
        assert_eq!(activity.cc.unwrap(), json!([]));
    }
}
