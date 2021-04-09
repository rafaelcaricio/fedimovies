use std::collections::HashSet;

use ammonia::Builder;

pub fn clean_html(unsafe_html: &str) -> String {
    let mut allowed_tags = HashSet::new();
    allowed_tags.insert("a");
    allowed_tags.insert("br");

    let safe_html = Builder::default()
        .tags(allowed_tags)
        .clean(unsafe_html)
        .to_string();
    safe_html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_html() {
        let unsafe_html = r#"<p>test <b>bold</b><script>dangerous</script> with <a href="https://example.com">link</a></p>"#;
        let safe_html = clean_html(unsafe_html);
        assert_eq!(safe_html, r#"test bold with <a href="https://example.com" rel="noopener noreferrer">link</a>"#);
    }
}
