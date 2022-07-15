use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::{create_activity, Activity, Attachment, Tag},
    actor::Actor,
    constants::{AP_CONTEXT, AP_PUBLIC},
    deliverer::OutgoingActivity,
    identifiers::{
        local_actor_id,
        local_actor_followers,
        local_actor_subscribers,
        local_object_id,
    },
    vocabulary::{CREATE, DOCUMENT, HASHTAG, MENTION, NOTE},
};
use crate::config::Instance;
use crate::errors::DatabaseError;
use crate::frontend::get_tag_page_url;
use crate::models::posts::queries::get_post_author;
use crate::models::posts::types::{Post, Visibility};
use crate::models::profiles::types::DbActorProfile;
use crate::models::relationships::queries::{get_followers, get_subscribers};
use crate::models::users::types::User;
use crate::utils::files::get_file_url;

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

pub fn build_note(
    instance_host: &str,
    instance_url: &str,
    post: &Post,
    subscribers: Vec<DbActorProfile>,
) -> Note {
    let object_id = local_object_id(instance_url, &post.id);
    let actor_id = local_actor_id(instance_url, &post.author.username);
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
    let followers_collection_id =
        local_actor_followers(instance_url, &post.author.username);
    let subscribers_collection_id =
        local_actor_subscribers(instance_url, &post.author.username);
    match post.visibility {
        Visibility::Public => {
            primary_audience.push(AP_PUBLIC.to_string());
            secondary_audience.push(followers_collection_id);
        },
        Visibility::Followers => {
            primary_audience.push(followers_collection_id);
        },
        Visibility::Subscribers => {
            primary_audience.push(subscribers_collection_id);
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
    if matches!(post.visibility, Visibility::Subscribers) {
        // Mention all subscribers
        // (for recipients that don't support subscribers-only posts)
        for profile in subscribers {
            let tag_name = format!("@{}", profile.actor_address(instance_host));
            let actor_id = profile.actor_id(instance_url);
            secondary_audience.push(actor_id.clone());
            let tag = Tag {
                name: Some(tag_name),
                tag_type: MENTION.to_string(),
                href: Some(actor_id),
            };
            tags.push(tag);
        };
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

pub fn build_create_note(
    instance_host: &str,
    instance_url: &str,
    post: &Post,
    subscribers: Vec<DbActorProfile>,
) -> Activity {
    let object = build_note(instance_host, instance_url, post, subscribers);
    let primary_audience = object.to.clone();
    let secondary_audience = object.cc.clone();
    let activity_id = format!("{}/create", object.id);
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

pub async fn get_note_recipients(
    db_client: &impl GenericClient,
    current_user: &User,
    post: &Post,
) -> Result<Vec<Actor>, DatabaseError> {
    let mut audience = vec![];
    match post.visibility {
        Visibility::Public | Visibility::Followers => {
            let followers = get_followers(db_client, &current_user.id, None, None).await?;
            audience.extend(followers);
        },
        Visibility::Subscribers => {
            let subscribers = get_subscribers(db_client, &current_user.id).await?;
            audience.extend(subscribers);
        },
        Visibility::Direct => (),
    };
    if let Some(in_reply_to_id) = post.in_reply_to_id {
        // TODO: use post.in_reply_to ?
        let in_reply_to_author = get_post_author(db_client, &in_reply_to_id).await?;
        audience.push(in_reply_to_author);
    };
    audience.extend(post.mentions.clone());

    let mut recipients: Vec<Actor> = Vec::new();
    for profile in audience {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    Ok(recipients)
}

pub async fn prepare_create_note(
    db_client: &impl GenericClient,
    instance: Instance,
    author: &User,
    post: &Post,
) -> Result<OutgoingActivity, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let subscribers = if matches!(post.visibility, Visibility::Subscribers) {
        get_subscribers(db_client, &author.id).await?
    } else {
        vec![]
    };
    let activity = build_create_note(
        &instance.host(),
        &instance.url(),
        post,
        subscribers,
    );
    let recipients = get_note_recipients(db_client, author, post).await?;
    Ok(OutgoingActivity {
        instance,
        sender: author.clone(),
        activity,
        recipients,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    const INSTANCE_HOST: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_note() {
        let author = DbActorProfile {
            username: "author".to_string(),
            ..Default::default()
        };
        let post = Post { author, ..Default::default() };
        let note = build_note(INSTANCE_HOST, INSTANCE_URL, &post, vec![]);

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
            local_actor_followers(INSTANCE_URL, "author"),
        ]);
    }

    #[test]
    fn test_build_note_followers_only() {
        let post = Post {
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let note = build_note(INSTANCE_HOST, INSTANCE_URL, &post, vec![]);

        assert_eq!(note.to, vec![
            local_actor_followers(INSTANCE_URL, &post.author.username),
        ]);
        assert_eq!(note.cc.is_empty(), true);
    }

    #[test]
    fn test_build_note_with_local_parent() {
        let parent = Post::default();
        let post = Post {
            in_reply_to_id: Some(parent.id),
            in_reply_to: Some(Box::new(parent.clone())),
            ..Default::default()
        };
        let note = build_note(INSTANCE_HOST, INSTANCE_URL, &post, vec![]);

        assert_eq!(
            note.in_reply_to.unwrap(),
            format!("{}/objects/{}", INSTANCE_URL, parent.id),
        );
        assert_eq!(note.to, vec![
            AP_PUBLIC.to_string(),
            local_actor_id(INSTANCE_URL, &parent.author.username),
        ]);
    }

    #[test]
    fn test_build_note_with_remote_parent() {
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
        let note = build_note(INSTANCE_HOST, INSTANCE_URL, &post, vec![]);

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
    fn test_build_create_note() {
        let author_username = "author";
        let author = DbActorProfile {
            username: author_username.to_string(),
            ..Default::default()
        };
        let post = Post { author, ..Default::default() };
        let activity = build_create_note(
            INSTANCE_HOST,
            INSTANCE_URL,
            &post,
            vec![],
        );

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
}
