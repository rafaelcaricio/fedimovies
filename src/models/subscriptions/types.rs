use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

#[derive(FromSql)]
#[postgres(name = "subscription")]
pub struct DbSubscription {
    pub id: i32,
    pub sender_id: Uuid,
    #[allow(dead_code)]
    sender_address: String,
    pub recipient_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
