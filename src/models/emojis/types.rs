use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::json_macro::{json_from_sql, json_to_sql};
use super::validators::EMOJI_MAX_SIZE;

fn default_emoji_file_size() -> usize { EMOJI_MAX_SIZE }

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(test, derive(Default))]
pub struct EmojiImage {
    pub file_name: String,
    #[serde(default = "default_emoji_file_size")]
    pub file_size: usize,
    pub media_type: String,
}

json_from_sql!(EmojiImage);
json_to_sql!(EmojiImage);

#[derive(Clone, FromSql)]
#[cfg_attr(test, derive(Default))]
#[postgres(name = "emoji")]
pub struct DbEmoji {
    pub id: Uuid,
    pub emoji_name: String,
    pub hostname: Option<String>,
    pub image: EmojiImage,
    pub object_id: Option<String>,
    pub updated_at: DateTime<Utc>,
}
