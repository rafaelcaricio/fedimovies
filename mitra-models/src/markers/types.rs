use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseTypeError,
};

#[derive(Debug)]
pub enum Timeline {
    Home,
    Notifications,
}

impl From<&Timeline> for i16 {
    fn from(value: &Timeline) -> i16 {
        match value {
            Timeline::Home => 1,
            Timeline::Notifications => 2,
        }
    }
}

impl TryFrom<i16> for Timeline {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let timeline = match value {
            1 => Self::Home,
            2 => Self::Notifications,
            _ => return Err(DatabaseTypeError),
        };
        Ok(timeline)
    }
}

int_enum_from_sql!(Timeline);
int_enum_to_sql!(Timeline);

#[allow(dead_code)]
#[derive(FromSql)]
#[postgres(name = "timeline_marker")]
pub struct DbTimelineMarker {
    id: i32,
    user_id: Uuid,
    pub timeline: Timeline,
    pub last_read_id: String,
    pub updated_at: DateTime<Utc>,
}
