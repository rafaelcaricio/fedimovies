use std::collections::HashMap;

use regex::{Captures, Match, Regex};

use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::{Post, Visibility},
};

use crate::activitypub::fetcher::helpers::get_post_by_object_id;

// MediaWiki-like syntax: [[url|text]]
const OBJECT_LINK_SEARCH_RE: &str = r"(?m)\[\[(?P<url>[^\s\|]+)(\|(?P<text>.+?))?\]\]";

pub fn is_inside_code_block(match_: &Match, text: &str) -> bool {
    // TODO: remove workaround.
    // Perform replacement only inside text nodes during markdown parsing
    let text_before = &text[0..match_.start()];
    let code_open = text_before.matches("<code>").count();
    let code_closed = text_before.matches("</code>").count();
    code_open > code_closed
}

/// Finds everything that looks like an object link
fn find_object_links(text: &str) -> Vec<String> {
    let link_re = Regex::new(OBJECT_LINK_SEARCH_RE).unwrap();
    let mut links = vec![];
    for caps in link_re.captures_iter(text) {
        let url_match = caps.name("url").expect("should have url group");
        if is_inside_code_block(&url_match, text) {
            // Ignore links inside code blocks
            continue;
        };
        let url = caps["url"].to_string();
        if !links.contains(&url) {
            links.push(url);
        };
    };
    links
}

pub async fn find_linked_posts(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    text: &str,
) -> Result<HashMap<String, Post>, DatabaseError> {
    let links = find_object_links(text);
    let mut link_map: HashMap<String, Post> = HashMap::new();
    let mut counter = 0;
    for url in links {
        if counter > 10 {
            // Limit the number of queries
            break;
            // TODO: single database query
        };
        match get_post_by_object_id(
            db_client,
            instance_url,
            &url,
        ).await {
            Ok(post) => {
                if post.repost_of_id.is_some() {
                    // Can't reference reposts
                    continue;
                };
                if post.visibility != Visibility::Public {
                    // Can't reference non-public posts
                    continue;
                };
                link_map.insert(url, post);
            },
            // If post doesn't exist in database, link is ignored
            Err(DatabaseError::NotFound(_)) => continue,
            Err(other_error) => return Err(other_error),
        };
        counter += 1;
    };
    Ok(link_map)
}

pub fn replace_object_links(
    link_map: &HashMap<String, Post>,
    text: &str,
) -> String {
    let mention_re = Regex::new(OBJECT_LINK_SEARCH_RE).unwrap();
    let result = mention_re.replace_all(text, |caps: &Captures| {
        let url_match = caps.name("url").expect("should have url group");
        if is_inside_code_block(&url_match, text) {
            // Don't replace inside code blocks
            return caps[0].to_string();
        };
        let url = caps["url"].to_string();
        let link_text = caps.name("text")
            .map(|match_| match_.as_str())
            .unwrap_or(&url)
            .to_string();
        if link_map.contains_key(&url) {
            return format!(r#"<a href="{0}">{1}</a>"#, url, link_text);
        };
        // Leave unchanged if post does not exist
        caps[0].to_string()
    });
    result.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_WITH_OBJECT_LINKS: &str = concat!(
        "test [[https://example.org/1]] link ",
        "test link with [[https://example.org/1|text]] ",
        "test ([[https://example.org/2]])",
    );

    #[test]
    fn test_is_inside_code_block() {
        let text = "abc<code>&&</code>xyz";
        let regexp = Regex::new("&&").unwrap();
        let mat = regexp.find(text).unwrap();
        assert_eq!(mat.start(), 9);
        let result = is_inside_code_block(&mat, text);
        assert_eq!(result, true);
    }

    #[test]
    fn test_find_object_links() {
        let results = find_object_links(TEXT_WITH_OBJECT_LINKS);
        assert_eq!(results, vec![
            "https://example.org/1",
            "https://example.org/2",
        ]);
    }

    #[test]
    fn test_replace_object_links() {
        let mut link_map = HashMap::new();
        link_map.insert("https://example.org/1".to_string(), Post::default());
        link_map.insert("https://example.org/2".to_string(), Post::default());
        let result = replace_object_links(&link_map, TEXT_WITH_OBJECT_LINKS);
        let expected_result = concat!(
            r#"test <a href="https://example.org/1">https://example.org/1</a> link "#,
            r#"test link with <a href="https://example.org/1">text</a> "#,
            r#"test (<a href="https://example.org/2">https://example.org/2</a>)"#,
        );
        assert_eq!(result, expected_result);
    }
}
