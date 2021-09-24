use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::media::types::Attachment;
use crate::models::posts::types::{Post, PostCreateData};

/// https://docs.joinmastodon.org/entities/status/
#[derive(Serialize)]
pub struct Status {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub account: Account,
    pub content: String,
    pub in_reply_to_id: Option<Uuid>,
    pub replies_count: i32,
    pub media_attachments: Vec<Attachment>,

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
        let account = Account::from_profile(post.author, instance_url);
        Self {
            id: post.id,
            created_at: post.created_at,
            account: account,
            content: post.content,
            in_reply_to_id: post.in_reply_to_id,
            replies_count: post.reply_count,
            media_attachments: attachments,
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
            attachments: value.media_ids.unwrap_or(vec![]),
            created_at: None,
        }
    }
}
