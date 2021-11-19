use std::convert::TryFrom;

use postgres_types::FromSql;
use serde::Serialize;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::database::int_enum::{int_enum_from_sql, int_enum_to_sql};
use crate::errors::ConversionError;

#[derive(Serialize)]
pub struct Relationship {
    pub id: Uuid,
    pub following: bool,
    pub followed_by: bool,
    pub requested: bool,
}

impl TryFrom<&Row> for Relationship {

    type Error = tokio_postgres::Error;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let relationship = Relationship {
            id: row.try_get("id")?,
            following: row.try_get("following")?,
            followed_by: row.try_get("followed_by")?,
            requested: row.try_get("requested")?,
        };
        Ok(relationship)
    }
}

#[derive(Debug)]
pub enum FollowRequestStatus {
    Pending,
    Accepted,
    Rejected,
}

impl From<&FollowRequestStatus> for i16 {
    fn from(value: &FollowRequestStatus) -> i16 {
        match value {
            FollowRequestStatus::Pending  => 1,
            FollowRequestStatus::Accepted => 2,
            FollowRequestStatus::Rejected => 3,
        }
    }
}

impl TryFrom<i16> for FollowRequestStatus {
    type Error = ConversionError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let status = match value {
            1 => Self::Pending,
            2 => Self::Accepted,
            3 => Self::Rejected,
            _ => return Err(ConversionError),
        };
        Ok(status)
    }
}

int_enum_from_sql!(FollowRequestStatus);
int_enum_to_sql!(FollowRequestStatus);

#[derive(FromSql)]
#[postgres(name = "follow_request")]
pub struct DbFollowRequest {
    pub id: Uuid,
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub request_status: FollowRequestStatus,
}
