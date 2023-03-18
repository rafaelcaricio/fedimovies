use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::activitypub::identifiers::{
    local_tag_collection,
    post_object_id,
};
use crate::mastodon_api::{
    accounts::types::Account,
    custom_emojis::types::CustomEmoji,
    media::types::Attachment,
};
use crate::models::{
    emojis::types::DbEmoji,
    posts::types::{Post, Visibility},
    profiles::types::DbActorProfile,
};

/// https://docs.joinmastodon.org/entities/mention/
#[derive(Serialize)]
pub struct Mention {
    id: String,
    username: String,
    acct: String,
    url: String,
}

impl Mention {
    fn from_profile(instance_url: &str, profile: DbActorProfile) -> Self {
        Mention {
            id: profile.id.to_string(),
            username: profile.username.clone(),
            acct: profile.acct.clone(),
            url: profile.actor_url(instance_url),
        }
    }
}

/// https://docs.joinmastodon.org/entities/tag/
#[derive(Serialize)]
pub struct Tag {
    name: String,
    url: String,
}

impl Tag {
    pub fn from_tag_name(instance_url: &str, tag_name: String) -> Self {
        let tag_url = local_tag_collection(instance_url, &tag_name);
        Tag {
            name: tag_name,
            url: tag_url,
        }
    }
}

/// https://docs.joinmastodon.org/entities/status/
#[derive(Serialize)]
pub struct Status {
    pub id: Uuid,
    pub uri: String,
    pub created_at: DateTime<Utc>,
    // Undocumented https://github.com/mastodon/mastodon/blob/v3.5.2/app/serializers/rest/status_serializer.rb
    edited_at: Option<DateTime<Utc>>,
    pub account: Account,
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub reblog: Option<Box<Status>>,
    pub visibility: String,
    pub spoiler_text: Option<String>,
    pub replies_count: i32,
    pub favourites_count: i32,
    pub reblogs_count: i32,
    pub media_attachments: Vec<Attachment>,
    mentions: Vec<Mention>,
    tags: Vec<Tag>,
    emojis: Vec<CustomEmoji>,

    // Authorized user attributes
    pub favourited: bool,
    pub reblogged: bool,

    // Extra fields
    pub ipfs_cid: Option<String>,
    pub token_id: Option<i32>,
    pub token_tx_id: Option<String>,
    links: Vec<Status>,
}

impl Status {
    pub fn from_post(
        base_url: &str,
        instance_url: &str,
        post: Post,
    ) -> Self {
        let object_id = post_object_id(instance_url, &post);
        let attachments: Vec<Attachment> = post.attachments.into_iter()
            .map(|item| Attachment::from_db(base_url, item))
            .collect();
        let mentions: Vec<Mention> = post.mentions.into_iter()
            .map(|item| Mention::from_profile(instance_url, item))
            .collect();
        let tags: Vec<Tag> = post.tags.into_iter()
            .map(|tag_name| Tag::from_tag_name(instance_url, tag_name))
            .collect();
        let emojis: Vec<CustomEmoji> = post.emojis.into_iter()
            .map(|emoji| CustomEmoji::from_db(base_url, emoji))
            .collect();
        let account = Account::from_profile(
            base_url,
            instance_url,
            post.author,
        );
        let reblog = if let Some(repost_of) = post.repost_of {
            let status = Status::from_post(base_url, instance_url, *repost_of);
            Some(Box::new(status))
        } else {
            None
        };
        let links = post.linked.into_iter().map(|post| {
            Status::from_post(base_url, instance_url, post)
        }).collect();
        let visibility = match post.visibility {
            Visibility::Public => "public",
            Visibility::Direct => "direct",
            Visibility::Followers => "private",
            Visibility::Subscribers => "subscribers",
        };
        Self {
            id: post.id,
            uri: object_id,
            created_at: post.created_at,
            edited_at: post.updated_at,
            account: account,
            content: post.content,
            in_reply_to_id: post.in_reply_to_id,
            reblog: reblog,
            visibility: visibility.to_string(),
            spoiler_text: None,
            replies_count: post.reply_count,
            favourites_count: post.reaction_count,
            reblogs_count: post.repost_count,
            media_attachments: attachments,
            mentions: mentions,
            tags: tags,
            emojis: emojis,
            favourited: post.actions.as_ref().map_or(false, |actions| actions.favourited),
            reblogged: post.actions.as_ref().map_or(false, |actions| actions.reposted),
            ipfs_cid: post.ipfs_cid,
            token_id: post.token_id,
            token_tx_id: post.token_tx_id,
            links: links,
        }
    }
}

fn default_post_content_type() -> String { "text/html".to_string() }

/// https://docs.joinmastodon.org/methods/statuses/
#[derive(Deserialize)]
pub struct StatusData {
    pub status: String,

    #[serde(rename = "media_ids[]")]
    pub media_ids: Option<Vec<Uuid>>,

    pub in_reply_to_id: Option<Uuid>,
    pub visibility: Option<String>,

    // Not supported by Mastodon
    pub mentions: Option<Vec<Uuid>>,

    #[serde(default = "default_post_content_type")]
    pub content_type: String,
}

#[derive(Deserialize)]
pub struct StatusPreviewData {
    pub status: String,

    #[serde(default = "default_post_content_type")]
    pub content_type: String,
}

#[derive(Serialize)]
pub struct StatusPreview {
    pub content: String,
    pub emojis: Vec<CustomEmoji>
}

impl StatusPreview {
    pub fn new(
        instance_url: &str,
        content: String,
        emojis: Vec<DbEmoji>,
    ) -> Self {
        let emojis: Vec<CustomEmoji> = emojis.into_iter()
            .map(|emoji| CustomEmoji::from_db(instance_url, emoji))
            .collect();
        Self { content, emojis }
    }
}

#[derive(Serialize)]
pub struct Context {
    pub ancestors: Vec<Status>,
    pub descendants: Vec<Status>,
}

#[derive(Deserialize)]
pub struct TransactionData {
    pub transaction_id: String,
}
