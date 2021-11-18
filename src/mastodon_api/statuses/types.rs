use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::media::types::Attachment;
use crate::models::posts::types::{Post, PostCreateData, Visibility};
use crate::models::profiles::types::DbActorProfile;

/// https://docs.joinmastodon.org/entities/mention/
#[derive(Serialize)]
pub struct Mention {
    id: String,
    username: String,
    acct: String,
    url: String,
}

impl Mention {
    fn from_profile(profile: DbActorProfile, instance_url: &str) -> Self {
        Mention {
            id: profile.id.to_string(),
            username: profile.username.clone(),
            acct: profile.acct.clone(),
            url: profile.actor_id(instance_url).unwrap(),
        }
    }
}

/// https://docs.joinmastodon.org/entities/status/
#[derive(Serialize)]
pub struct Status {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub account: Account,
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub visibility: String,
    pub replies_count: i32,
    pub favourites_count: i32,
    pub media_attachments: Vec<Attachment>,
    mentions: Vec<Mention>,

    // Authorized user attributes
    pub favourited: bool,

    // Extra fields
    pub ipfs_cid: Option<String>,
    pub token_id: Option<i32>,
    pub token_tx_id: Option<String>,
}

impl Status {
    pub fn from_post(post: Post, instance_url: &str) -> Self {
        let attachments: Vec<Attachment> = post.attachments.into_iter()
            .map(|item| Attachment::from_db(item, instance_url))
            .collect();
        let mentions: Vec<Mention> = post.mentions.into_iter()
            .map(|item| Mention::from_profile(item, instance_url))
            .collect();
        let account = Account::from_profile(post.author, instance_url);
        let visibility = match post.visibility {
            Visibility::Public => "public",
            Visibility::Direct => "direct",
        };
        Self {
            id: post.id,
            created_at: post.created_at,
            account: account,
            content: post.content,
            in_reply_to_id: post.in_reply_to_id,
            visibility: visibility.to_string(),
            replies_count: post.reply_count,
            favourites_count: post.reaction_count,
            media_attachments: attachments,
            mentions: mentions,
            favourited: post.actions.map_or(false, |actions| actions.favourited),
            ipfs_cid: post.ipfs_cid,
            token_id: post.token_id,
            token_tx_id: post.token_tx_id,
        }
    }
}

/// https://docs.joinmastodon.org/methods/statuses/
#[derive(Deserialize)]
pub struct StatusData {
    pub status: String,

    #[serde(rename = "media_ids[]")]
    pub media_ids: Option<Vec<Uuid>>,

    pub in_reply_to_id: Option<Uuid>,
}

impl From<StatusData> for PostCreateData {

    fn from(value: StatusData) -> Self {
        Self {
            content: value.status,
            in_reply_to_id: value.in_reply_to_id,
            visibility: Visibility::Public,
            attachments: value.media_ids.unwrap_or(vec![]),
            mentions: vec![],
            object_id: None,
            created_at: None,
        }
    }
}
