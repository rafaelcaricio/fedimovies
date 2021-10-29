use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::statuses::types::Status;
use crate::models::notifications::types::{EventType, Notification};

/// https://docs.joinmastodon.org/entities/notification/
#[derive(Serialize)]
pub struct ApiNotification {
    pub id: String,

    #[serde(rename = "type")]
    pub event_type: String,

    pub created_at: DateTime<Utc>,

    pub account: Account,
    pub status: Option<Status>,
}

impl ApiNotification {
    pub fn from_db(notification: Notification, instance_url: &str) -> Self {
        let account = Account::from_profile(
            notification.sender,
            instance_url,
        );
        let status = notification.post.map(|post| {
            Status::from_post(post, instance_url)
        });
        let event_type_mastodon = match notification.event_type {
            EventType::Follow => "follow",
            EventType::FollowRequest => "follow_request",
            EventType::Reply => "reply",
            EventType::Reaction => "favourite",
        };
        Self {
            id: notification.id.to_string(),
            event_type: event_type_mastodon.to_string(),
            created_at: notification.created_at,
            account,
            status,
        }
    }
}
