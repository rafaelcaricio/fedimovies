use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::media::get_file_url;
use crate::models::attachments::types::{
    AttachmentType,
    DbMediaAttachment,
};

#[derive(Deserialize)]
pub struct AttachmentCreateData {
    // base64-encoded file (not comtaible with Mastodon)
    pub file: String,
    pub media_type: Option<String>,
}

/// https://docs.joinmastodon.org/entities/attachment/
#[derive(Serialize)]
pub struct Attachment {
    pub id: Uuid,

    #[serde(rename = "type")]
    pub attachment_type: String,

    pub url: String,
}

impl Attachment {
    pub fn from_db(instance_url: &str, db_attachment: DbMediaAttachment) -> Self {
        let attachment_type =
            AttachmentType::from_media_type(db_attachment.media_type);
        let attachment_type_mastodon = match attachment_type {
            AttachmentType::Unknown => "unknown",
            AttachmentType::Image => "image",
            AttachmentType::Video => "video",
        };
        let attachment_url = get_file_url(
            instance_url,
            &db_attachment.file_name,
        );
        Self {
            id: db_attachment.id,
            attachment_type: attachment_type_mastodon.to_string(),
            url: attachment_url,
        }
    }
}
