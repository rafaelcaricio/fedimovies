use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::attachments::types::DbMediaAttachment;
use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseError, DatabaseTypeError,
};
use crate::emojis::types::DbEmoji;
use crate::posts::types::{DbPost, Post};
use crate::profiles::types::DbActorProfile;

#[derive(Debug)]
pub enum EventType {
    Follow,
    FollowRequest,
    Reply,
    Reaction,
    Mention,
    Repost,
    Subscription,
    SubscriptionStart,
    SubscriptionExpiration,
    Move,
}

impl From<&EventType> for i16 {
    fn from(value: &EventType) -> i16 {
        match value {
            EventType::Follow => 1,
            EventType::FollowRequest => 2,
            EventType::Reply => 3,
            EventType::Reaction => 4,
            EventType::Mention => 5,
            EventType::Repost => 6,
            EventType::Subscription => 7,
            EventType::SubscriptionStart => unimplemented!("not supported"),
            EventType::SubscriptionExpiration => 9,
            EventType::Move => 10,
        }
    }
}

impl TryFrom<i16> for EventType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let event_type = match value {
            1 => Self::Follow,
            2 => Self::FollowRequest,
            3 => Self::Reply,
            4 => Self::Reaction,
            5 => Self::Mention,
            6 => Self::Repost,
            7 => Self::Subscription,
            8 => Self::SubscriptionStart,
            9 => Self::SubscriptionExpiration,
            10 => Self::Move,
            _ => return Err(DatabaseTypeError),
        };
        Ok(event_type)
    }
}

int_enum_from_sql!(EventType);
int_enum_to_sql!(EventType);

#[allow(dead_code)]
#[derive(FromSql)]
#[postgres(name = "notification")]
struct DbNotification {
    id: i32,
    sender_id: Uuid,
    recipient_id: Uuid,
    post_id: Option<Uuid>,
    event_type: EventType,
    created_at: DateTime<Utc>,
}

pub struct Notification {
    pub id: i32,
    pub sender: DbActorProfile,
    pub recipient: DbActorProfile,
    pub post: Option<Post>,
    pub event_type: EventType,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<&Row> for Notification {
    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_notification: DbNotification = row.try_get("notification")?;
        let db_sender: DbActorProfile = row.try_get("sender")?;
        let db_recipient: DbActorProfile = row.try_get("recipient")?;
        let maybe_db_post: Option<DbPost> = row.try_get("post")?;
        let maybe_post = match maybe_db_post {
            Some(db_post) => {
                let db_post_author: DbActorProfile = row.try_get("post_author")?;
                let db_attachments: Vec<DbMediaAttachment> = row.try_get("attachments")?;
                let db_mentions: Vec<DbActorProfile> = row.try_get("mentions")?;
                let db_tags: Vec<String> = row.try_get("tags")?;
                let db_links: Vec<Uuid> = row.try_get("links")?;
                let db_emojis: Vec<DbEmoji> = row.try_get("emojis")?;
                let post = Post::new(
                    db_post,
                    db_post_author,
                    db_attachments,
                    db_mentions,
                    db_tags,
                    db_links,
                    db_emojis,
                )?;
                Some(post)
            }
            None => None,
        };
        let notification = Self {
            id: db_notification.id,
            sender: db_sender,
            recipient: db_recipient,
            post: maybe_post,
            event_type: db_notification.event_type,
            created_at: db_notification.created_at,
        };
        Ok(notification)
    }
}
