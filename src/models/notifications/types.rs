use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::errors::{ConversionError, DatabaseError};
use crate::models::profiles::types::DbActorProfile;

#[allow(dead_code)]
#[derive(FromSql)]
#[postgres(name = "notification")]
struct DbNotification {
    id: i32,
    sender_id: Uuid,
    recipient_id: Uuid,
    post_id: Option<Uuid>,
    event_type: i16,
    created_at: DateTime<Utc>,
}

pub enum EventType {
    Follow,
}

impl From<EventType> for i16 {
    fn from(value: EventType) -> i16 {
        match value {
            EventType::Follow => 1,
        }
    }
}

impl TryFrom<i16> for EventType {
    type Error = ConversionError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let event_type = match value {
            1 => Self::Follow,
            _ => return Err(ConversionError),
        };
        Ok(event_type)
    }
}

pub struct Notification {
    pub id: i32,
    pub sender: DbActorProfile,
    pub event_type: EventType,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<&Row> for Notification {

    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_notification: DbNotification = row.try_get("notification")?;
        let db_sender: DbActorProfile = row.try_get("sender")?;
        let notification = Self {
            id: db_notification.id,
            sender: db_sender,
            event_type: EventType::try_from(db_notification.event_type)?,
            created_at: db_notification.created_at,
        };
        Ok(notification)
    }
}
