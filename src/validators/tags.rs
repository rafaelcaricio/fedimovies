use regex::Regex;

use crate::errors::ValidationError;

const HASHTAG_NAME_RE: &str = r"^\w+$";

pub fn validate_hashtag(tag_name: &str) -> Result<(), ValidationError> {
    let hashtag_name_re = Regex::new(HASHTAG_NAME_RE).unwrap();
    if !hashtag_name_re.is_match(tag_name) {
        return Err(ValidationError("invalid tag name".to_string()));
    };
    Ok(())
}
