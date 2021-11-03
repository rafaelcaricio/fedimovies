use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use postgres_protocol::types::{int2_from_sql, int2_to_sql};
use postgres_types::{
    FromSql, ToSql, IsNull, Type,
    accepts, to_sql_checked,
    private::BytesMut,
};
use uuid::Uuid;

use crate::errors::ConversionError;

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
    type Error = ConversionError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let timeline = match value {
            1 => Self::Home,
            2 => Self::Notifications,
            _ => return Err(ConversionError),
        };
        Ok(timeline)
    }
}

type SqlError = Box<dyn std::error::Error + Sync + Send>;

impl<'a> FromSql<'a> for Timeline {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Timeline, SqlError> {
        let int_value = int2_from_sql(raw)?;
        let timeline = Timeline::try_from(int_value)?;
        Ok(timeline)
    }

    accepts!(INT2);
}

impl ToSql for Timeline {
    fn to_sql(&self, _: &Type, out: &mut BytesMut) -> Result<IsNull, SqlError> {
        let int_value: i16 = self.into();
        int2_to_sql(int_value, out);
        Ok(IsNull::No)
    }

    accepts!(INT2);
    to_sql_checked!();
}

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
