use crate::errors::ValidationError;
use crate::utils::html::clean_html_strict;

pub const ATTACHMENTS_MAX_NUM: usize = 15;
pub const CONTENT_MAX_SIZE: usize = 100000;
const CONTENT_ALLOWED_TAGS: [&str; 8] = [
    "a",
    "br",
    "pre",
    "code",
    "strong",
    "em",
    "p",
    "span",
];

pub fn content_allowed_classes() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("a", vec!["hashtag", "mention", "u-url"]),
        ("span", vec!["h-card"]),
        ("p", vec!["inline-quote"]),
    ]
}

pub fn clean_content(
    content: &str,
) -> Result<String, ValidationError> {
    // Check content size to not exceed the hard limit
    // Character limit from config is not enforced at the backend
    if content.len() > CONTENT_MAX_SIZE {
        return Err(ValidationError("post is too long"));
    };
    let content_safe = clean_html_strict(
        content,
        &CONTENT_ALLOWED_TAGS,
        content_allowed_classes(),
    );
    let content_trimmed = content_safe.trim();
    if content_trimmed.is_empty() {
        return Err(ValidationError("post can not be empty"));
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
        assert_eq!(result.is_ok(), false);
    }

    #[test]
    fn test_clean_content_trimming() {
        let content = "test ";
        let cleaned = clean_content(content).unwrap();
        assert_eq!(cleaned, "test");
    }
}
