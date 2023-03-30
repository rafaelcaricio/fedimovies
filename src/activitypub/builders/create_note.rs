use chrono::{DateTime, Utc};
use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    emojis::types::DbEmoji,
    posts::queries::get_post_author,
    posts::types::{Post, Visibility},
    profiles::types::DbActor,
    relationships::queries::{get_followers, get_subscribers},
    users::types::User,
};

use crate::activitypub::{
    constants::{AP_MEDIA_TYPE, AP_PUBLIC},
    deliverer::OutgoingActivity,
    identifiers::{
        local_actor_id,
        local_actor_followers,
        local_actor_subscribers,
        local_emoji_id,
        local_object_id,
        local_tag_collection,
        post_object_id,
        profile_actor_id,
    },
    types::{
        build_default_context,
        Attachment,
        Context,
        EmojiTag,
        EmojiTagImage,
        LinkTag,
        SimpleTag,
    },
    vocabulary::*,
};
use crate::media::get_file_url;
use crate::webfinger::types::ActorAddress;

#[allow(dead_code)]
#[derive(Serialize)]
#[serde(untagged)]
enum Tag {
    SimpleTag(SimpleTag),
    LinkTag(LinkTag),
    EmojiTag(EmojiTag),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    #[serde(rename = "@context")]
    context: Context,

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

    pub to: Vec<String>,
    pub cc: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    quote_url: Option<String>,
}

pub fn build_emoji_tag(instance_url: &str, emoji: &DbEmoji) -> EmojiTag {
    EmojiTag {
        tag_type: EMOJI.to_string(),
        icon: EmojiTagImage {
            object_type: IMAGE.to_string(),
            url: get_file_url(instance_url, &emoji.image.file_name),
            media_type: Some(emoji.image.media_type.clone()),
        },
        id: local_emoji_id(instance_url, &emoji.emoji_name),
        name: format!(":{}:", emoji.emoji_name),
        updated: emoji.updated_at,
    }
}

pub fn build_note(
    instance_hostname: &str,
    instance_url: &str,
    post: &Post,
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
        let actor_address = ActorAddress::from_profile(
            instance_hostname,
            profile,
        );
        let tag_name = format!("@{}", actor_address);
        let actor_id = profile_actor_id(instance_url, profile);
        if !primary_audience.contains(&actor_id) {
            primary_audience.push(actor_id.clone());
        };
        let tag = SimpleTag {
            name: tag_name,
            tag_type: MENTION.to_string(),
            href: actor_id,
        };
        tags.push(Tag::SimpleTag(tag));
    };
    for tag_name in &post.tags {
        let tag_href = local_tag_collection(instance_url, tag_name);
        let tag = SimpleTag {
            name: format!("#{}", tag_name),
            tag_type: HASHTAG.to_string(),
            href: tag_href,
        };
        tags.push(Tag::SimpleTag(tag));
    };

    assert_eq!(post.links.len(), post.linked.len());
    for linked in &post.linked {
        // Build FEP-e232 object link
        // https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-e232.md
        let link_href = post_object_id(instance_url, linked);
        let tag = LinkTag {
            name: None,  // no microsyntax
            tag_type: LINK.to_string(),
            href: link_href,
            media_type: AP_MEDIA_TYPE.to_string(),
        };
        if cfg!(feature = "fep-e232") {
            tags.push(Tag::LinkTag(tag));
        };
    };
    let maybe_quote_url = post.linked.get(0)
        .map(|linked| post_object_id(instance_url, linked));

    for emoji in &post.emojis {
        let tag = build_emoji_tag(instance_url, emoji);
        tags.push(Tag::EmojiTag(tag));
    };

    let in_reply_to_object_id = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            let in_reply_to = post.in_reply_to.as_ref()
                .expect("in_reply_to should be populated");
            assert_eq!(in_reply_to.id, in_reply_to_id);
            let in_reply_to_actor_id = profile_actor_id(
                instance_url,
                &in_reply_to.author,
            );
            if !primary_audience.contains(&in_reply_to_actor_id) {
                primary_audience.push(in_reply_to_actor_id);
            };
            Some(post_object_id(instance_url, in_reply_to))
        },
        None => None,
    };
    Note {
        context: build_default_context(),
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
        quote_url: maybe_quote_url,
    }
}

#[derive(Serialize)]
pub struct CreateNote {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Note,

    to: Vec<String>,
    cc: Vec<String>,
}

pub fn build_create_note(
    instance_hostname: &str,
    instance_url: &str,
    post: &Post,
) -> CreateNote {
    let object = build_note(instance_hostname, instance_url, post);
    let primary_audience = object.to.clone();
    let secondary_audience = object.cc.clone();
    let activity_id = format!("{}/create", object.id);
    CreateNote {
        context: build_default_context(),
        activity_type: CREATE.to_string(),
        id: activity_id,
        actor: object.attributed_to.clone(),
        object: object,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn get_note_recipients(
    db_client: &impl DatabaseClient,
    current_user: &User,
    post: &Post,
) -> Result<Vec<DbActor>, DatabaseError> {
    let mut audience = vec![];
    match post.visibility {
        Visibility::Public | Visibility::Followers => {
            let followers = get_followers(db_client, &current_user.id).await?;
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

    let mut recipients = vec![];
    for profile in audience {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    Ok(recipients)
}

pub async fn prepare_create_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    author: &User,
    post: &Post,
) -> Result<OutgoingActivity, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let activity = build_create_note(
        &instance.hostname(),
        &instance.url(),
        post,
    );
    let recipients = get_note_recipients(db_client, author, post).await?;
    Ok(OutgoingActivity::new(
        instance,
        author,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_HOSTNAME: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_tag() {
        let simple_tag = SimpleTag {
            tag_type: HASHTAG.to_string(),
            href: "https://example.org/tags/test".to_string(),
            name: "#test".to_string(),
        };
        let tag = Tag::SimpleTag(simple_tag);
        let value = serde_json::to_value(tag).unwrap();
        assert_eq!(value, json!({
            "type": "Hashtag",
            "href": "https://example.org/tags/test",
            "name": "#test",
        }));
    }

    #[test]
    fn test_build_emoji_tag() {
        let emoji = DbEmoji {
            emoji_name: "test".to_string(),
            ..Default::default()
        };
        let emoji_tag = build_emoji_tag(INSTANCE_URL, &emoji);
        assert_eq!(emoji_tag.id, "https://example.com/objects/emojis/test");
        assert_eq!(emoji_tag.name, ":test:");
    }

    #[test]
    fn test_build_note() {
        let author = DbActorProfile {
            username: "author".to_string(),
            ..Default::default()
        };
        let post = Post {
            author,
            tags: vec!["test".to_string()],
            ..Default::default()
        };
        let note = build_note(INSTANCE_HOSTNAME, INSTANCE_URL, &post);

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
        assert_eq!(note.tag.len(), 1);
        let tag = match note.tag[0] {
            Tag::SimpleTag(ref tag) => tag,
            _ => panic!(),
        };
        assert_eq!(tag.name, "#test");
        assert_eq!(tag.href, "https://example.com/collections/tags/test");
    }

    #[test]
    fn test_build_note_followers_only() {
        let post = Post {
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let note = build_note(INSTANCE_HOSTNAME, INSTANCE_URL, &post);

        assert_eq!(note.to, vec![
            local_actor_followers(INSTANCE_URL, &post.author.username),
        ]);
        assert_eq!(note.cc.is_empty(), true);
    }

    #[test]
    fn test_build_note_subscribers_only() {
        let subscriber_id = "https://test.com/users/3";
        let subscriber = DbActorProfile {
            username: "subscriber".to_string(),
            hostname: Some("test.com".to_string()),
            actor_json: Some(DbActor {
                id: subscriber_id.to_string(),
                ..Default::default()
            }),
            actor_id: Some(subscriber_id.to_string()),
            ..Default::default()
        };
        let post = Post {
            visibility: Visibility::Subscribers,
            mentions: vec![subscriber],
            ..Default::default()
        };
        let note = build_note(INSTANCE_HOSTNAME, INSTANCE_URL, &post);

        assert_eq!(note.to, vec![
            local_actor_subscribers(INSTANCE_URL, &post.author.username),
            subscriber_id.to_string(),
        ]);
        assert_eq!(note.cc.is_empty(), true);
    }

    #[test]
    fn test_build_note_direct() {
        let mentioned_id = "https://test.com/users/3";
        let mentioned = DbActorProfile {
            username: "mention".to_string(),
            hostname: Some("test.com".to_string()),
            actor_json: Some(DbActor {
                id: mentioned_id.to_string(),
                ..Default::default()
            }),
            actor_id: Some(mentioned_id.to_string()),
            ..Default::default()
        };
        let post = Post {
            visibility: Visibility::Direct,
            mentions: vec![mentioned],
            ..Default::default()
        };
        let note = build_note(INSTANCE_HOSTNAME, INSTANCE_URL, &post);

        assert_eq!(note.to, vec![mentioned_id]);
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
        let note = build_note(INSTANCE_HOSTNAME, INSTANCE_URL, &post);

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
            username: "test".to_string(),
            hostname: Some("test.net".to_string()),
            acct: parent_author_acct.to_string(),
            actor_json: Some(DbActor {
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
        let note = build_note(INSTANCE_HOSTNAME, INSTANCE_URL, &post);

        assert_eq!(
            note.in_reply_to.unwrap(),
            parent.object_id.unwrap(),
        );
        let tags = note.tag;
        assert_eq!(tags.len(), 1);
        let tag = match tags[0] {
            Tag::SimpleTag(ref tag) => tag,
            _ => panic!(),
        };
        assert_eq!(tag.name, format!("@{}", parent_author_acct));
        assert_eq!(tag.href, parent_author_actor_id);
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
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &post,
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
        assert_eq!(activity.to, vec![AP_PUBLIC]);
        assert_eq!(activity.object.attributed_to, activity.actor);
        assert_eq!(activity.object.to, activity.to);
        assert_eq!(activity.object.cc, activity.cc);
    }
}
