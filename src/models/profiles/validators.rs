use regex::Regex;
use crate::errors::ValidationError;
use crate::utils::html::{clean_html, clean_html_strict};
use super::types::ExtraField;

const USERNAME_RE: &str = r"^[a-zA-Z0-9_\.-]+$";
const DISPLAY_NAME_MAX_LENGTH: usize = 200;
const BIO_MAX_LENGTH: usize = 10000;
const BIO_ALLOWED_TAGS: [&str; 2] = ["a", "br"];
const FIELD_NAME_MAX_SIZE: usize = 500;
const FIELD_VALUE_MAX_SIZE: usize = 5000;

pub fn validate_username(username: &str) -> Result<(), ValidationError> {
    if username.is_empty() {
        return Err(ValidationError("username is empty"));
    };
    if username.len() > 100 {
        return Err(ValidationError("username is too long"));
    };
    let username_regexp = Regex::new(USERNAME_RE).unwrap();
    if !username_regexp.is_match(username) {
        return Err(ValidationError("invalid username"));
    };
    Ok(())
}

pub fn validate_display_name(display_name: &str)
    -> Result<(), ValidationError>
{
    if display_name.chars().count() > DISPLAY_NAME_MAX_LENGTH {
        return Err(ValidationError("display name is too long"));
    };
    Ok(())
}

pub fn clean_bio(bio: &str, is_remote: bool) -> Result<String, ValidationError> {
    let cleaned_bio = if is_remote {
        // Remote profile
        let truncated_bio: String = bio.chars().take(BIO_MAX_LENGTH).collect();
        clean_html(&truncated_bio, vec![])
    } else {
        // Local profile
        if bio.chars().count() > BIO_MAX_LENGTH {
            return Err(ValidationError("bio is too long"));
        };
        clean_html_strict(bio, &BIO_ALLOWED_TAGS, vec![])
    };
    Ok(cleaned_bio)
}

/// Validates extra fields and removes fields with empty labels
pub fn clean_extra_fields(extra_fields: &[ExtraField])
    -> Result<Vec<ExtraField>, ValidationError>
{
    let mut cleaned_extra_fields = vec![];
    for mut field in extra_fields.iter().cloned() {
        field.name = field.name.trim().to_string();
        field.value = clean_html_strict(&field.value, &BIO_ALLOWED_TAGS, vec![]);
        if field.name.is_empty() {
            continue;
        };
        if field.name.len() > FIELD_NAME_MAX_SIZE {
            return Err(ValidationError("field name is too long"));
        };
        if field.value.len() > FIELD_VALUE_MAX_SIZE {
            return Err(ValidationError("field value is too long"));
        };
        cleaned_extra_fields.push(field);
    };
    if cleaned_extra_fields.len() > 20 {
        return Err(ValidationError("at most 20 fields are allowed"));
    };
    Ok(cleaned_extra_fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_username() {
        let result_1 = validate_username("test");
        assert!(result_1.is_ok());
        let result_2 = validate_username("test_12-3.xyz");
        assert!(result_2.is_ok());
    }

    #[test]
    fn test_validate_username_error() {
        let error = validate_username(&"x".repeat(101)).unwrap_err();
        assert_eq!(error.to_string(), "username is too long");
        let error = validate_username("").unwrap_err();
        assert_eq!(error.to_string(), "username is empty");
        let error = validate_username("abc&").unwrap_err();
        assert_eq!(error.to_string(), "invalid username");
    }

    #[test]
    fn test_validate_display_name() {
        let result_1 = validate_display_name("test");
        assert!(result_1.is_ok());

        let result_2 = validate_display_name(&"x".repeat(201));
        assert!(result_2.is_err());
    }

    #[test]
    fn test_clean_bio() {
        let bio = "test\n<script>alert()</script>123";
        let result = clean_bio(bio, true).unwrap();
        assert_eq!(result, "test\n123");
    }

    #[test]
    fn test_clean_extra_fields() {
        let extra_fields = vec![ExtraField {
            name: " $ETH ".to_string(),
            value: "<p>0x1234</p>".to_string(),
            value_source: None,
        }];
        let result = clean_extra_fields(&extra_fields).unwrap().pop().unwrap();
        assert_eq!(result.name, "$ETH");
        assert_eq!(result.value, "0x1234");
    }
}
