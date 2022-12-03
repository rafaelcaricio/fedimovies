use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::database::DatabaseError;
use crate::models::profiles::types::DbActorProfile;
use crate::utils::caip2::ChainId;

#[derive(FromSql)]
#[postgres(name = "subscription")]
pub struct DbSubscription {
    pub id: i32,
    pub sender_id: Uuid,
    pub sender_address: Option<String>,
    pub recipient_id: Uuid,
    pub chain_id: ChainId,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct Subscription {
    pub id: i32,
    pub sender: DbActorProfile,
    pub sender_address: Option<String>,
    pub expires_at: DateTime<Utc>,
}

impl TryFrom<&Row> for Subscription {

    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_subscription: DbSubscription = row.try_get("subscription")?;
        let db_sender: DbActorProfile = row.try_get("sender")?;
        Ok(Self {
            id: db_subscription.id,
            sender: db_sender,
            sender_address: db_subscription.sender_address,
            expires_at: db_subscription.expires_at,
        })
    }
}
