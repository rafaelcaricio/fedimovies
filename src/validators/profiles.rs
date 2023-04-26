use regex::Regex;

use fedimovies_models::profiles::types::{ExtraField, ProfileCreateData, ProfileUpdateData};
use fedimovies_utils::html::{clean_html, clean_html_strict};

use crate::errors::ValidationError;

use super::posts::EMOJI_LIMIT;

const USERNAME_RE: &str = r"^[a-zA-Z0-9_\.-]+$";
const DISPLAY_NAME_MAX_LENGTH: usize = 200;
const BIO_MAX_LENGTH: usize = 10000;
const BIO_ALLOWED_TAGS: [&str; 2] = ["a", "br"];
const FIELD_NAME_MAX_SIZE: usize = 500;
const FIELD_VALUE_MAX_SIZE: usize = 5000;

pub fn validate_username(username: &str) -> Result<(), ValidationError> {
    if username.is_empty() {
        return Err(ValidationError("username is empty".to_string()));
    };
    if username.len() > 100 {
        return Err(ValidationError("username is too long".to_string()));
    };
    let username_regexp = Regex::new(USERNAME_RE).unwrap();
    if !username_regexp.is_match(username) {
        return Err(ValidationError("invalid username".to_string()));
    };
    Ok(())
}

fn validate_display_name(display_name: &str) -> Result<(), ValidationError> {
    if display_name.chars().count() > DISPLAY_NAME_MAX_LENGTH {
        return Err(ValidationError("display name is too long".to_string()));
    };
    Ok(())
}

fn clean_bio(bio: &str, is_remote: bool) -> Result<String, ValidationError> {
    let cleaned_bio = if is_remote {
        // Remote profile
        let truncated_bio: String = bio.chars().take(BIO_MAX_LENGTH).collect();
        clean_html(&truncated_bio, vec![])
    } else {
        // Local profile
        if bio.chars().count() > BIO_MAX_LENGTH {
            return Err(ValidationError("bio is too long".to_string()));
        };
        clean_html_strict(bio, &BIO_ALLOWED_TAGS, vec![])
    };
    Ok(cleaned_bio)
}

/// Validates extra fields and removes fields with empty labels
fn clean_extra_fields(
    extra_fields: &[ExtraField],
    is_remote: bool,
) -> Result<Vec<ExtraField>, ValidationError> {
    let mut cleaned_extra_fields = vec![];
    for mut field in extra_fields.iter().cloned() {
        field.name = field.name.trim().to_string();
        field.value = clean_html_strict(&field.value, &BIO_ALLOWED_TAGS, vec![]);
        if field.name.is_empty() {
            continue;
        };
        if field.name.len() > FIELD_NAME_MAX_SIZE {
            return Err(ValidationError("field name is too long".to_string()));
        };
        if field.value.len() > FIELD_VALUE_MAX_SIZE {
            return Err(ValidationError("field value is too long".to_string()));
        };
        cleaned_extra_fields.push(field);
    }
    #[allow(clippy::collapsible_else_if)]
    if is_remote {
        if cleaned_extra_fields.len() > 100 {
            return Err(ValidationError(
                "at most 100 fields are allowed".to_string(),
            ));
        };
    } else {
        if cleaned_extra_fields.len() > 10 {
            return Err(ValidationError("at most 10 fields are allowed".to_string()));
        };
    };
    Ok(cleaned_extra_fields)
}

pub fn clean_profile_create_data(
    profile_data: &mut ProfileCreateData,
) -> Result<(), ValidationError> {
    validate_username(&profile_data.username)?;
    if profile_data.hostname.is_some() != profile_data.actor_json.is_some() {
        return Err(ValidationError(
            "hostname and actor_json field mismatch".to_string(),
        ));
    };
    if let Some(display_name) = &profile_data.display_name {
        validate_display_name(display_name)?;
    };
    let is_remote = profile_data.actor_json.is_some();
    if let Some(bio) = &profile_data.bio {
        let cleaned_bio = clean_bio(bio, is_remote)?;
        profile_data.bio = Some(cleaned_bio);
    };
    profile_data.extra_fields = clean_extra_fields(&profile_data.extra_fields, is_remote)?;
    if profile_data.emojis.len() > EMOJI_LIMIT {
        return Err(ValidationError("too many emojis".to_string()));
    };
    Ok(())
}

pub fn clean_profile_update_data(
    profile_data: &mut ProfileUpdateData,
) -> Result<(), ValidationError> {
    if let Some(display_name) = &profile_data.display_name {
        validate_display_name(display_name)?;
    };
    let is_remote = profile_data.actor_json.is_some();
    if let Some(bio) = &profile_data.bio {
        let cleaned_bio = clean_bio(bio, is_remote)?;
        profile_data.bio = Some(cleaned_bio);
    };
    profile_data.extra_fields = clean_extra_fields(&profile_data.extra_fields, is_remote)?;
    if profile_data.emojis.len() > EMOJI_LIMIT {
        return Err(ValidationError("too many emojis".to_string()));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fedimovies_models::profiles::types::DbActor;

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
        let result = clean_extra_fields(&extra_fields, false)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(result.name, "$ETH");
        assert_eq!(result.value, "0x1234");
    }

    #[test]
    fn test_clean_profile_create_data() {
        let mut profile_data = ProfileCreateData {
            username: "test".to_string(),
            hostname: Some("example.org".to_string()),
            display_name: Some("Test Test".to_string()),
            actor_json: Some(DbActor {
                id: "https://example.org/test".to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = clean_profile_create_data(&mut profile_data);
        assert_eq!(result.is_ok(), true);
    }
}
