use regex::{Captures, Regex};

use crate::errors::ConversionError;
use crate::frontend::get_tag_page_url;

const HASHTAG_RE: &str = r"(?m)(?P<before>^|\s)#(?P<tag>\S+)";
const HASHTAG_SECONDARY_RE: &str = r"^(?P<tag>[0-9A-Za-z]+)(?P<after>(\.|<br>|\.<br>)?)$";
const HASHTAG_NAME_RE: &str = r"^\w+$";

/// Finds anything that looks like a hashtag
pub fn find_tags(text: &str) -> Vec<String> {
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
pub fn replace_tags(instance_url: &str, text: &str, tags: &[String]) -> String {
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
                format!(
                    r#"{}<a class="hashtag" href="{}">#{}</a>{}"#,
                    before,
                    tag_page_url,
                    tag,
                    after,
                )
            } else {
                caps[0].to_string()
            }
        } else {
            caps[0].to_string()
        }
    });
    result.to_string()
}

pub fn normalize_tag(tag: &str) -> Result<String, ConversionError> {
    let hashtag_name_re = Regex::new(HASHTAG_NAME_RE).unwrap();
    let tag_name = tag.trim_start_matches('#');
    if !hashtag_name_re.is_match(tag_name) {
        return Err(ConversionError);
    };
    Ok(tag_name.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_find_tags() {
        let text = concat!(
            "@user1@server1 some text #TestTag.\n",
            "#TAG1 #tag1 #test_underscore #test*special ",
            "more text #tag2",
        );
        let tags = find_tags(text);

        assert_eq!(tags, vec![
            "testtag",
            "tag1",
            "tag2",
        ]);
    }

    #[test]
    fn test_replace_tags() {
        let text = concat!(
            "@user1@server1 some text #TestTag.\n",
            "#TAG1 #tag1 #test_underscore #test*special ",
            "more text #tag2",
        );
        let tags = find_tags(text);
        let output = replace_tags(INSTANCE_URL, &text, &tags);

        let expected_output = concat!(
            r#"@user1@server1 some text <a class="hashtag" href="https://example.com/tag/testtag">#TestTag</a>."#, "\n",
            r#"<a class="hashtag" href="https://example.com/tag/tag1">#TAG1</a> <a class="hashtag" href="https://example.com/tag/tag1">#tag1</a> "#,
            r#"#test_underscore #test*special "#,
            r#"more text <a class="hashtag" href="https://example.com/tag/tag2">#tag2</a>"#,
        );
        assert_eq!(output, expected_output);
    }

    #[test]
    fn test_normalize_tag() {
        let tag = "#ActivityPub";
        let output = normalize_tag(tag).unwrap();

        assert_eq!(output, "activitypub");
    }
}
