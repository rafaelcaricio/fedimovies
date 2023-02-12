use std::collections::HashMap;

use regex::Regex;

use crate::database::{DatabaseClient, DatabaseError};
use crate::models::{
    emojis::queries::get_local_emojis_by_names,
    emojis::types::DbEmoji,
};
use super::links::is_inside_code_block;

// See also: EMOJI_NAME_RE in models::emojis::validators
const SHORTCODE_SEARCH_RE: &str = r"(?m):(?P<name>[\w.]+):";

/// Finds emoji shortcodes in text
fn find_shortcodes(text: &str) -> Vec<String> {
    let shortcode_re = Regex::new(SHORTCODE_SEARCH_RE)
        .expect("regex should be valid");
    let mut emoji_names = vec![];
    for caps in shortcode_re.captures_iter(text) {
        let name_match = caps.name("name").expect("should have name group");
        if is_inside_code_block(&name_match, text) {
            // Ignore shortcodes inside code blocks
            continue;
        };
        let name = caps["name"].to_string();
        if !emoji_names.contains(&name) {
            emoji_names.push(name);
        };
    };
    emoji_names
}

pub async fn find_emojis(
    db_client: &impl DatabaseClient,
    text: &str,
) -> Result<HashMap<String, DbEmoji>, DatabaseError> {
    let emoji_names = find_shortcodes(text);
    // If shortcode doesn't exist in database, it is ignored
    let emojis = get_local_emojis_by_names(db_client, &emoji_names).await?;
    let mut emoji_map: HashMap<String, DbEmoji> = HashMap::new();
    for emoji in emojis {
        emoji_map.insert(emoji.emoji_name.clone(), emoji);
    };
    Ok(emoji_map)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_WITH_EMOJIS: &str = "@user1@server1 text :emoji_name: :abc:";

    #[test]
    fn test_find_shortcodes() {
        let emoji_names = find_shortcodes(TEXT_WITH_EMOJIS);

        assert_eq!(emoji_names, vec![
            "emoji_name",
            "abc",
        ]);
    }
}
