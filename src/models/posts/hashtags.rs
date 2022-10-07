use regex::{Captures, Regex};

use crate::errors::ValidationError;
use crate::frontend::get_tag_page_url;

const HASHTAG_RE: &str = r"(?m)(?P<before>^|\s|>|[\(])#(?P<tag>[^\s<]+)";
const HASHTAG_SECONDARY_RE: &str = r"^(?P<tag>[0-9A-Za-z]+)(?P<after>[\.,:?\)]?)$";
const HASHTAG_NAME_RE: &str = r"^\w+$";

/// Finds anything that looks like a hashtag
pub fn find_hashtags(text: &str) -> Vec<String> {
    let hashtag_re = Regex::new(HASHTAG_RE).unwrap();
    let hashtag_secondary_re = Regex::new(HASHTAG_SECONDARY_RE).unwrap();
    let mut tags = vec![];
    for caps in hashtag_re.captures_iter(text) {
        if let Some(secondary_caps) = hashtag_secondary_re.captures(&caps["tag"]) {
            let tag_name = secondary_caps["tag"].to_string().to_lowercase();
            if !tags.contains(&tag_name) {
                tags.push(tag_name);
            };
        };
    };
    tags
}

/// Replaces hashtags with links
pub fn replace_hashtags(instance_url: &str, text: &str, tags: &[String]) -> String {
    let hashtag_re = Regex::new(HASHTAG_RE).unwrap();
    let hashtag_secondary_re = Regex::new(HASHTAG_SECONDARY_RE).unwrap();
    let result = hashtag_re.replace_all(text, |caps: &Captures| {
        if let Some(secondary_caps) = hashtag_secondary_re.captures(&caps["tag"]) {
            let before = caps["before"].to_string();
            let tag = secondary_caps["tag"].to_string();
            let tag_name = tag.to_lowercase();
            let after = secondary_caps["after"].to_string();
            if tags.contains(&tag_name) {
                let tag_page_url = get_tag_page_url(instance_url, &tag_name);
                return format!(
                    r#"{}<a class="hashtag" href="{}">#{}</a>{}"#,
                    before,
                    tag_page_url,
                    tag,
                    after,
                );
            };
        };
        caps[0].to_string()
    });
    result.to_string()
}

pub fn normalize_hashtag(tag: &str) -> Result<String, ValidationError> {
    let hashtag_name_re = Regex::new(HASHTAG_NAME_RE).unwrap();
    let tag_name = tag.trim_start_matches('#');
    if !hashtag_name_re.is_match(tag_name) {
        return Err(ValidationError("invalid tag name"));
    };
    Ok(tag_name.to_lowercase())
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

        assert_eq!(tags, vec![
            "testtag",
            "tag1",
            "tag2",
            "tag3",
            "tag4",
            "tag5",
        ]);
    }

    #[test]
    fn test_replace_hashtags() {
        let tags = find_hashtags(TEXT_WITH_TAGS);
        let output = replace_hashtags(INSTANCE_URL, TEXT_WITH_TAGS, &tags);

        let expected_output = concat!(
            r#"@user1@server1 some text <a class="hashtag" href="https://example.com/tag/testtag">#TestTag</a>."#, "\n",
            r#"<a class="hashtag" href="https://example.com/tag/tag1">#TAG1</a> <a class="hashtag" href="https://example.com/tag/tag1">#tag1</a> "#,
            r#"#test_underscore #test*special "#,
            r#"more text (<a class="hashtag" href="https://example.com/tag/tag2">#tag2</a>) text "#,
            r#"<a class="hashtag" href="https://example.com/tag/tag3">#tag3</a>, "#,
            r#"<a class="hashtag" href="https://example.com/tag/tag4">#tag4</a>:<br>"#,
            r#"end with <a class="hashtag" href="https://example.com/tag/tag5">#tag5</a>"#,
        );
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_normalize_hashtag() {
        let tag = "#ActivityPub";
        let output = normalize_hashtag(tag).unwrap();

        assert_eq!(output, "activitypub");
    }
}
