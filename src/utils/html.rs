use std::collections::HashSet;

use ammonia::Builder;

pub fn clean_html(unsafe_html: &str) -> String {
    let safe_html = Builder::default()
        .add_generic_attributes(&["class"])
        .add_tag_attributes("a", &["rel", "target"])
        .link_rel(None)
        .clean(unsafe_html)
        .to_string();
    safe_html
}

pub fn clean_html_strict(unsafe_html: &str) -> String {
    let mut allowed_tags = HashSet::new();
    allowed_tags.insert("a");
    allowed_tags.insert("br");
    allowed_tags.insert("pre");
    allowed_tags.insert("code");

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
        let unsafe_html = r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="ugc">@<span>user</span></a></span> test</p>"#;
        let safe_html = clean_html(unsafe_html);
        assert_eq!(safe_html, r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="ugc">@<span>user</span></a></span> test</p>"#);
    }

    #[test]
    fn test_clean_html_strict() {
        let unsafe_html = r#"<p>test <b>bold</b><script>dangerous</script> with <a href="https://example.com">link</a> and <code>code</code></p>"#;
        let safe_html = clean_html_strict(unsafe_html);
        assert_eq!(safe_html, r#"test bold with <a href="https://example.com" rel="noopener noreferrer">link</a> and <code>code</code>"#);
    }
}
