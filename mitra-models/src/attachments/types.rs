use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

#[derive(Clone, FromSql)]
#[postgres(name = "media_attachment")]
pub struct DbMediaAttachment {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub file_name: String,
    pub file_size: Option<i32>,
    pub media_type: Option<String>,
    pub ipfs_cid: Option<String>,
    pub post_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub enum AttachmentType {
    Unknown,
    Image,
    Video,
    Audio,
}

impl AttachmentType {
    pub fn from_media_type(value: Option<String>) -> Self {
        match value {
            Some(media_type) => {
                if media_type.starts_with("image/") {
                    Self::Image
                } else if media_type.starts_with("video/") {
                    Self::Video
                } else if media_type.starts_with("audio/") {
                    Self::Audio
                } else {
                    Self::Unknown
                }
            }
            None => Self::Unknown,
        }
    }
}
