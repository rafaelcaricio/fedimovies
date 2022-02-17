use crate::errors::ValidationError;
use crate::utils::html::{clean_html, clean_html_strict};

pub fn validate_username(username: &str) -> Result<(), ValidationError> {
    if username.len() > 100 {
        return Err(ValidationError("username is too long"));
    };
    Ok(())
}

pub fn validate_display_name(display_name: &str)
    -> Result<(), ValidationError>
{
    if display_name.len() > 200 {
        return Err(ValidationError("display name is too long"));
    };
    Ok(())
}

const BIO_MAX_SIZE: usize = 10000;

pub fn clean_bio(bio: &str, is_remote: bool) -> Result<String, ValidationError> {
    if bio.len() > BIO_MAX_SIZE {
        return Err(ValidationError("bio is too long"));
    };
    let cleaned_bio = if is_remote {
        // Remote profile
        clean_html(bio)
    } else {
        // Local profile
        clean_html_strict(bio)
    };
    Ok(cleaned_bio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_username() {
        let result_1 = validate_username("test");
        assert!(result_1.is_ok());
        let result_2 = validate_username(&"x".repeat(101));
        assert!(result_2.is_err());
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
}
