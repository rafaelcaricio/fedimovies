use std::collections::HashMap;

use regex::{Captures, Regex};
use tokio_postgres::GenericClient;

use crate::errors::DatabaseError;
use super::helpers::get_post_by_object_id;
use super::types::Post;

const OBJECT_LINK_SEARCH_RE: &str = r"(?m)\[\[(?P<url>\S+)\]\]";

/// Finds everything that looks like an object link
fn find_object_links(text: &str) -> Vec<String> {
    let link_re = Regex::new(OBJECT_LINK_SEARCH_RE).unwrap();
    let mut links = vec![];
    for caps in link_re.captures_iter(text) {
        let url = caps["url"].to_string();
        if !links.contains(&url) {
            links.push(url);
        };
    };
    links
}

pub async fn find_linked_posts(
    db_client: &impl GenericClient,
    instance_url: &str,
    text: &str,
) -> Result<HashMap<String, Post>, DatabaseError> {
    let links = find_object_links(text);
    let mut link_map: HashMap<String, Post> = HashMap::new();
    for url in links {
        // Return error if post doesn't exist
        let post = get_post_by_object_id(
            db_client,
            instance_url,
            &url,
        ).await?;
        link_map.insert(url, post);
    };
    Ok(link_map)
}

pub fn replace_object_links(
    link_map: &HashMap<String, Post>,
    text: &str,
) -> String {
    let mention_re = Regex::new(OBJECT_LINK_SEARCH_RE).unwrap();
    let result = mention_re.replace_all(text, |caps: &Captures| {
        let url = caps["url"].to_string();
        if link_map.contains_key(&url) {
            return format!(r#"<a href="{0}">{0}</a>"#, url);
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
        "test ([[https://example.org/2]])",
    );

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
            r#"test (<a href="https://example.org/2">https://example.org/2</a>)"#,
        );
        assert_eq!(result, expected_result);
    }
}
