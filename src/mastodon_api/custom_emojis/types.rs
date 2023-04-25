use serde::Serialize;

use fedimovies_models::emojis::types::DbEmoji;

use crate::media::get_file_url;

/// https://docs.joinmastodon.org/entities/CustomEmoji/
#[derive(Serialize)]
pub struct CustomEmoji {
    shortcode: String,
    url: String,
    static_url: String,
    visible_in_picker: bool,
}

impl CustomEmoji {
    pub fn from_db(base_url: &str, emoji: DbEmoji) -> Self {
        let image_url = get_file_url(base_url, &emoji.image.file_name);
        Self {
            shortcode: emoji.emoji_name,
            url: image_url.clone(),
            static_url: image_url,
            visible_in_picker: true,
        }
    }
}
