use std::collections::{HashMap, HashSet};
use std::iter::FromIterator;

use ammonia::Builder;

pub use ammonia::clean_text as escape_html;

pub fn clean_html(
    unsafe_html: &str,
    allowed_classes: Vec<(&'static str, Vec<&'static str>)>,
) -> String {
    let mut builder = Builder::default();
    for (tag, classes) in allowed_classes.iter() {
        builder.add_allowed_classes(tag, classes);
    }
    let safe_html = builder
        // Remove src from external images to prevent tracking
        .set_tag_attribute_value("img", "src", "")
        // Always add rel="noopener"
        .link_rel(Some("noopener"))
        .clean(unsafe_html)
        .to_string();
    safe_html
}

pub fn clean_html_strict(
    unsafe_html: &str,
    allowed_tags: &[&str],
    allowed_classes: Vec<(&'static str, Vec<&'static str>)>,
) -> String {
    let allowed_tags = HashSet::from_iter(allowed_tags.iter().copied());
    let mut allowed_classes_map = HashMap::new();
    for (tag, classes) in allowed_classes {
        allowed_classes_map.insert(tag, HashSet::from_iter(classes.into_iter()));
    }
    let safe_html = Builder::default()
        .tags(allowed_tags)
        .allowed_classes(allowed_classes_map)
        .link_rel(Some("noopener"))
        .clean(unsafe_html)
        .to_string();
    safe_html
}

pub fn clean_html_all(html: &str) -> String {
    let text = Builder::empty().clean(html).to_string();
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_html() {
        let unsafe_html = concat!(
            r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="ugc">@<span>user</span></a></span> test</p>"#,
            r#"<p><img src="https://example.com/image.png" class="picture"></p>"#,
        );
        let expected_safe_html = concat!(
            r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="noopener">@<span>user</span></a></span> test</p>"#,
            r#"<p><img src=""></p>"#,
        );
        let safe_html = clean_html(
            unsafe_html,
            vec![("a", vec!["mention", "u-url"]), ("span", vec!["h-card"])],
        );
        assert_eq!(safe_html, expected_safe_html);
    }

    #[test]
    fn test_clean_html_strict() {
        let unsafe_html = r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="ugc">@<span>user</span></a></span> test <b>bold</b><script>dangerous</script> with <a href="https://example.com" target="_blank" rel="noopener">link</a> and <code>code</code></p>"#;
        let safe_html = clean_html_strict(
            unsafe_html,
            &["a", "br", "code", "p", "span"],
            vec![("a", vec!["mention", "u-url"]), ("span", vec!["h-card"])],
        );
        assert_eq!(
            safe_html,
            r#"<p><span class="h-card"><a href="https://example.com/user" class="u-url mention" rel="noopener">@<span>user</span></a></span> test bold with <a href="https://example.com" rel="noopener">link</a> and <code>code</code></p>"#
        );
    }

    #[test]
    fn test_clean_html_all() {
        let html = r#"<p>test <b>bold</b><script>dangerous</script> with <a href="https://example.com">link</a> and <code>code</code></p>"#;
        let text = clean_html_all(html);
        assert_eq!(text, "test bold with link and code");
    }
}
