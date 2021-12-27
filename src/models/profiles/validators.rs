use crate::errors::ValidationError;

pub fn validate_username(username: &str) -> Result<(), ValidationError> {
    if username.len() > 100 {
        return Err(ValidationError("username is too long"));
    };
    Ok(())
}

pub fn validate_display_name(display_name: Option<&String>)
    -> Result<(), ValidationError>
{
    if let Some(display_name) = display_name {
        if display_name.len() > 200 {
            return Err(ValidationError("display name is too long"));
        };
    };
    Ok(())
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
        let display_name = "test".to_string();
        let result_1 = validate_display_name(Some(&display_name));
        assert!(result_1.is_ok());

        let result_2 = validate_display_name(None);
        assert!(result_2.is_ok());

        let display_name = "x".repeat(201);
        let result_3 = validate_display_name(Some(&display_name));
        assert!(result_3.is_err());
    }
}
