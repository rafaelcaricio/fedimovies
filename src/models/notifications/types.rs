use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::database::int_enum::{int_enum_from_sql, int_enum_to_sql};
use crate::errors::{ConversionError, DatabaseError};
use crate::models::attachments::types::DbMediaAttachment;
use crate::models::posts::types::{DbPost, Post};
use crate::models::profiles::types::DbActorProfile;

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

#[derive(Debug)]
pub enum EventType {
    Follow,
    FollowRequest,
    Reply,
    Reaction,
    Mention,
    Repost,
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
        }
    }
}

impl TryFrom<i16> for EventType {
    type Error = ConversionError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let event_type = match value {
            1 => Self::Follow,
            2 => Self::FollowRequest,
            3 => Self::Reply,
            4 => Self::Reaction,
            5 => Self::Mention,
            6 => Self::Repost,
            _ => return Err(ConversionError),
        };
        Ok(event_type)
    }
}

int_enum_from_sql!(EventType);
int_enum_to_sql!(EventType);

pub struct Notification {
    pub id: i32,
    pub sender: DbActorProfile,
    pub post: Option<Post>,
    pub event_type: EventType,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<&Row> for Notification {

    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_notification: DbNotification = row.try_get("notification")?;
        let db_sender: DbActorProfile = row.try_get("sender")?;
        let maybe_db_post: Option<DbPost> = row.try_get("post")?;
        let maybe_post = match maybe_db_post {
            Some(db_post) => {
                let db_post_author: DbActorProfile = row.try_get("post_author")?;
                let db_attachments: Vec<DbMediaAttachment> = row.try_get("attachments")?;
                let db_mentions: Vec<DbActorProfile> = row.try_get("mentions")?;
                let post = Post::new(db_post, db_post_author, db_attachments, db_mentions)?;
                Some(post)
            },
            None => None,
        };
        let notification = Self {
            id: db_notification.id,
            sender: db_sender,
            post: maybe_post,
            event_type: db_notification.event_type,
            created_at: db_notification.created_at,
        };
        Ok(notification)
    }
}
