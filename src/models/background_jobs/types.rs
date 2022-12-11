use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use serde_json::Value;
use postgres_types::FromSql;
use uuid::Uuid;

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseTypeError,
};

#[derive(Debug, PartialEq)]
pub enum JobType {
    IncomingActivity,
}

impl From<&JobType> for i16 {
    fn from(value: &JobType) -> i16 {
        match value {
            JobType::IncomingActivity => 1,
        }
    }
}

impl TryFrom<i16> for JobType {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let job_type = match value {
            1 => Self::IncomingActivity,
            _ => return Err(DatabaseTypeError),
        };
        Ok(job_type)
    }
}

int_enum_from_sql!(JobType);
int_enum_to_sql!(JobType);

#[derive(Debug, PartialEq)]
pub enum JobStatus {
    Queued,
    Running,
}

impl From<&JobStatus> for i16 {
    fn from(value: &JobStatus) -> i16 {
        match value {
            JobStatus::Queued => 1,
            JobStatus::Running => 2,
        }
    }
}

impl TryFrom<i16> for JobStatus {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let job_status = match value {
            1 => Self::Queued,
            2 => Self::Running,
            _ => return Err(DatabaseTypeError),
        };
        Ok(job_status)
    }
}

int_enum_from_sql!(JobStatus);
int_enum_to_sql!(JobStatus);

#[derive(FromSql)]
#[postgres(name = "background_job")]
pub struct DbBackgroundJob {
    pub id: Uuid,
    pub job_type: JobType,
    pub job_data: Value,
    pub job_status: JobStatus,
    pub scheduled_for: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
