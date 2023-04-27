use fedimovies_utils::html::clean_html_strict;

use crate::errors::ValidationError;

pub const ATTACHMENT_LIMIT: usize = 15;
pub const MENTION_LIMIT: usize = 50;
pub const LINK_LIMIT: usize = 10;
pub const EMOJI_LIMIT: usize = 50;

pub const OBJECT_ID_SIZE_MAX: usize = 2000;
pub const CONTENT_MAX_SIZE: usize = 100000;
const CONTENT_ALLOWED_TAGS: [&str; 8] = ["a", "br", "pre", "code", "strong", "em", "p", "span"];

pub fn content_allowed_classes() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("a", vec!["hashtag", "mention", "u-url"]),
        ("span", vec!["h-card"]),
        ("p", vec!["inline-quote"]),
    ]
}

pub fn clean_content(content: &str) -> Result<String, ValidationError> {
    // Check content size to not exceed the hard limit
    // Character limit from config is not enforced at the backend
    if content.len() > CONTENT_MAX_SIZE {
        return Err(ValidationError("post is too long".to_string()));
    };
    let content_safe = clean_html_strict(content, &CONTENT_ALLOWED_TAGS, content_allowed_classes());
    let content_trimmed = content_safe.trim();
    if content_trimmed.is_empty() {
        return Err(ValidationError("post can not be empty".to_string()));
    };
    Ok(content_trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_content_empty() {
        let content = "  ";
        let result = clean_content(content);
        assert!(!result.is_ok());
    }

    #[test]
    fn test_clean_content_trimming() {
        let content = "test ";
        let cleaned = clean_content(content).unwrap();
        assert_eq!(cleaned, "test");
    }
}
