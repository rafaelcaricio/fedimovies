use regex::{Captures, Regex};

use super::links::is_inside_code_block;
use crate::activitypub::identifiers::local_tag_collection;

// See also: HASHTAG_NAME_RE in validators::tags
const HASHTAG_RE: &str = r"(?m)(?P<before>^|\s|>|[\(])#(?P<tag>[^\s<]+)";
const HASHTAG_SECONDARY_RE: &str = r"^(?P<tag>[0-9A-Za-z]+)(?P<after>[\.,:?!\)]?)$";

/// Finds anything that looks like a hashtag
pub fn find_hashtags(text: &str) -> Vec<String> {
    let hashtag_re = Regex::new(HASHTAG_RE).unwrap();
    let hashtag_secondary_re = Regex::new(HASHTAG_SECONDARY_RE).unwrap();
    let mut tags = vec![];
    for caps in hashtag_re.captures_iter(text) {
        let tag_match = caps.name("tag").expect("should have tag group");
        if is_inside_code_block(&tag_match, text) {
            // Ignore hashtags inside code blocks
            continue;
        };
        if let Some(secondary_caps) = hashtag_secondary_re.captures(&caps["tag"]) {
            let tag_name = secondary_caps["tag"].to_string().to_lowercase();
            if !tags.contains(&tag_name) {
                tags.push(tag_name);
            };
        };
    }
    tags
}

/// Replaces hashtags with links
pub fn replace_hashtags(instance_url: &str, text: &str, tags: &[String]) -> String {
    let hashtag_re = Regex::new(HASHTAG_RE).unwrap();
    let hashtag_secondary_re = Regex::new(HASHTAG_SECONDARY_RE).unwrap();
    let result = hashtag_re.replace_all(text, |caps: &Captures| {
        let tag_match = caps.name("tag").expect("should have tag group");
        if is_inside_code_block(&tag_match, text) {
            // Don't replace hashtags inside code blocks
            return caps[0].to_string();
        };
        if let Some(secondary_caps) = hashtag_secondary_re.captures(&caps["tag"]) {
            let before = caps["before"].to_string();
            let tag = secondary_caps["tag"].to_string();
            let tag_name = tag.to_lowercase();
            let after = secondary_caps["after"].to_string();
            if tags.contains(&tag_name) {
                let tag_url = local_tag_collection(instance_url, &tag_name);
                return format!(
                    r#"{}<a class="hashtag" href="{}">#{}</a>{}"#,
                    before, tag_url, tag, after,
                );
            };
        };
        caps[0].to_string()
    });
    result.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";
    const TEXT_WITH_TAGS: &str = concat!(
        "@user1@server1 some text #TestTag.\n",
        "#TAG1 #tag1 #test_underscore #test*special ",
        "more text (#tag2) text #tag3, #tag4:<br>",
        "end with #tag5",
    );

    #[test]
    fn test_find_hashtags() {
        let tags = find_hashtags(TEXT_WITH_TAGS);

        assert_eq!(
            tags,
            vec!["testtag", "tag1", "tag2", "tag3", "tag4", "tag5",]
        );
    }

    #[test]
    fn test_replace_hashtags() {
        let tags = find_hashtags(TEXT_WITH_TAGS);
        let output = replace_hashtags(INSTANCE_URL, TEXT_WITH_TAGS, &tags);

        let expected_output = concat!(
            r#"@user1@server1 some text <a class="hashtag" href="https://example.com/collections/tags/testtag">#TestTag</a>."#,
            "\n",
            r#"<a class="hashtag" href="https://example.com/collections/tags/tag1">#TAG1</a> "#,
            r#"<a class="hashtag" href="https://example.com/collections/tags/tag1">#tag1</a> "#,
            r#"#test_underscore #test*special "#,
            r#"more text (<a class="hashtag" href="https://example.com/collections/tags/tag2">#tag2</a>) text "#,
            r#"<a class="hashtag" href="https://example.com/collections/tags/tag3">#tag3</a>, "#,
            r#"<a class="hashtag" href="https://example.com/collections/tags/tag4">#tag4</a>:<br>"#,
            r#"end with <a class="hashtag" href="https://example.com/collections/tags/tag5">#tag5</a>"#,
        );
        assert_eq!(output, expected_output);
    }
}
