use regex::Regex;

use crate::errors::ValidationError;

const EMOJI_NAME_RE: &str = r"^[a-zA-Z0-9._-]+$";
const EMOJI_NAME_SIZE_MAX: usize = 100; // database column limit
pub const EMOJI_MAX_SIZE: usize = 500 * 1000; // 500 kB
pub const EMOJI_LOCAL_MAX_SIZE: usize = 50 * 1000; // 50 kB
pub const EMOJI_MEDIA_TYPES: [&str; 4] = [
    "image/apng",
    "image/gif",
    "image/png",
    "image/webp",
];

pub fn validate_emoji_name(emoji_name: &str) -> Result<(), ValidationError> {
    let name_re = Regex::new(EMOJI_NAME_RE).unwrap();
    if !name_re.is_match(emoji_name) {
        return Err(ValidationError("invalid emoji name"));
    };
    if emoji_name.len() > EMOJI_NAME_SIZE_MAX {
        return Err(ValidationError("emoji name is too long"));
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

        let valid_name = "01-emoji-name";
        let result = validate_emoji_name(valid_name);
        assert!(result.is_ok());

        let invalid_name = "emoji\"<script>";
        let result = validate_emoji_name(invalid_name);
        assert!(result.is_err());
    }
}
