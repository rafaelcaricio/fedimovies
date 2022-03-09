use std::convert::TryFrom;

use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::database::int_enum::{int_enum_from_sql, int_enum_to_sql};
use crate::errors::ConversionError;

#[derive(Debug)]
pub enum RelationshipType {
    Follow,
    FollowRequest,
    Subscription,
    HideReposts,
    HideReplies,
}

impl From<&RelationshipType> for i16 {
    fn from(value: &RelationshipType) -> i16 {
        match value {
            RelationshipType::Follow => 1,
            RelationshipType::FollowRequest => 2,
            RelationshipType::Subscription => 3,
            RelationshipType::HideReposts => 4,
            RelationshipType::HideReplies => 5,
        }
    }
}

impl TryFrom<i16> for RelationshipType {
    type Error = ConversionError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let relationship_type = match value {
            1 => Self::Follow,
            2 => Self::FollowRequest,
            3 => Self::Subscription,
            4 => Self::HideReposts,
            5 => Self::HideReplies,
            _ => return Err(ConversionError),
        };
        Ok(relationship_type)
    }
}

int_enum_from_sql!(RelationshipType);
int_enum_to_sql!(RelationshipType);

pub struct DbRelationship {
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub relationship_type: RelationshipType,
}

impl DbRelationship {
    pub fn is_direct(
        &self,
        source_id: &Uuid,
        target_id: &Uuid,
    ) -> Result<bool, ConversionError> {
        if &self.source_id == source_id && &self.target_id == target_id {
            Ok(true)
        } else if &self.source_id == target_id && &self.target_id == source_id {
            Ok(false)
        } else {
            Err(ConversionError)
        }
    }
}

impl TryFrom<&Row> for DbRelationship {

    type Error = tokio_postgres::Error;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let relationship = Self {
            source_id: row.try_get("source_id")?,
            target_id: row.try_get("target_id")?,
            relationship_type: row.try_get("relationship_type")?,
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
