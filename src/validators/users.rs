use regex::Regex;

use crate::errors::ValidationError;

use super::profiles::validate_username;

pub fn validate_local_username(username: &str) -> Result<(), ValidationError> {
    validate_username(username)?;
    // The username regexp should not allow domain names and IP addresses
    let username_regexp = Regex::new(r"^[a-z0-9_]+$").unwrap();
    if !username_regexp.is_match(username) {
        return Err(ValidationError("invalid username"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_local_username() {
        let result_1 = validate_local_username("name_1");
        assert_eq!(result_1.is_ok(), true);
        let result_2 = validate_local_username("name&");
        assert_eq!(result_2.is_ok(), false);
    }
}
