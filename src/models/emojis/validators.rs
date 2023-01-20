use regex::Regex;

use crate::errors::ValidationError;

const EMOJI_NAME_RE: &str = r"^[\w.]+$";
pub const EMOJI_MAX_SIZE: usize = 250 * 1000; // 250 kB
pub const EMOJI_MEDIA_TYPES: [&str; 2] = [
    "image/gif",
    "image/png",
];

pub fn validate_emoji_name(emoji_name: &str) -> Result<(), ValidationError> {
    let name_re = Regex::new(EMOJI_NAME_RE).unwrap();
    if !name_re.is_match(emoji_name) {
        return Err(ValidationError("invalid emoji name"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_emoji_name() {
        let valid_name = "emoji_name";
        let result = validate_emoji_name(valid_name);
        assert!(result.is_ok());

        let invalid_name = "emoji\"<script>";
        let result = validate_emoji_name(invalid_name);
        assert!(result.is_err());
    }
}
