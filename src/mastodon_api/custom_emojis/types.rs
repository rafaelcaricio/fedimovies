use serde::Serialize;

use crate::media::get_file_url;
use crate::models::emojis::types::DbEmoji;

/// https://docs.joinmastodon.org/entities/CustomEmoji/
#[derive(Serialize)]
pub struct CustomEmoji {
    shortcode: String,
    url: String,
    static_url: String,
    visible_in_picker: bool,
}

impl CustomEmoji {
    pub fn from_db(instance_url: &str, emoji: DbEmoji) -> Self {
        let image_url = get_file_url(instance_url, &emoji.image.file_name);
        Self {
            shortcode: emoji.emoji_name,
            url: image_url.clone(),
            static_url: image_url,
            visible_in_picker: true,
        }
    }
}
