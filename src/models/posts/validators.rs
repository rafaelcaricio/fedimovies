use crate::errors::ValidationError;
use crate::utils::html::clean_html_strict;

const CONTENT_ALLOWED_TAGS: [&str; 6] = [
    "a",
    "br",
    "pre",
    "code",
    "strong",
    "em",
];

pub fn clean_content(
    content: &str,
    character_limit: usize,
) -> Result<String, ValidationError> {
    if content.chars().count() > character_limit {
        return Err(ValidationError("post is too long"));
    };
    let content_safe = clean_html_strict(content, &CONTENT_ALLOWED_TAGS);
    let content_trimmed = content_safe.trim();
    if content_trimmed.is_empty() {
        return Err(ValidationError("post can not be empty"));
    };
    Ok(content_trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const POST_CHARACTER_LIMIT: usize = 1000;

    #[test]
    fn test_clean_content_empty() {
        let content = "  ";
        let result = clean_content(content, POST_CHARACTER_LIMIT);
        assert_eq!(result.is_ok(), false);
    }

    #[test]
    fn test_clean_content_trimming() {
        let content = "test ";
        let cleaned = clean_content(content, POST_CHARACTER_LIMIT).unwrap();
        assert_eq!(cleaned, "test");
    }
}
