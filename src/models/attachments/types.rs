use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

#[derive(Clone, FromSql)]
#[postgres(name = "media_attachment")]
pub struct DbMediaAttachment {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub media_type: Option<String>,
    pub file_name: String,
    pub ipfs_cid: Option<String>,
    pub post_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub enum AttachmentType {
    Unknown,
    Image,
}

impl AttachmentType {
    pub fn from_media_type(value: Option<String>) -> Self {
        match value {
            Some(media_type) => {
                if media_type.starts_with("image/") {
                    Self::Image
                } else {
                    Self::Unknown
                }
            },
            None => Self::Unknown,
        }
    }
}
