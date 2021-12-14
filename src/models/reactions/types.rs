use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

#[derive(FromSql)]
#[postgres(name = "post_reaction")]
pub struct DbReaction {
    pub id: Uuid,
    pub author_id: Uuid,
    pub post_id: Uuid,
    pub activity_id: Option<String>,
    pub created_at: DateTime<Utc>,
}
