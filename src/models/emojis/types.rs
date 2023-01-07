use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

use crate::models::profiles::types::ProfileImage;

#[derive(Clone, FromSql)]
#[postgres(name = "emoji")]
pub struct DbEmoji {
    pub id: Uuid,
    pub emoji_name: String,
    pub hostname: Option<String>,
    pub image: ProfileImage,
    pub object_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}
