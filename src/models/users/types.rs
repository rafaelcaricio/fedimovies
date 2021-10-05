use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use regex::Regex;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ValidationError;
use crate::models::profiles::types::DbActorProfile;

#[derive(FromSql)]
#[postgres(name = "user_account")]
pub struct DbUser {
    pub id: Uuid,
    pub wallet_address: String,
    pub password_hash: String,
    pub private_key: String,
    pub invite_code: Option<String>,
    pub created_at: DateTime<Utc>,
}

// Represents local user
#[derive(Clone)]
pub struct User {
    pub id: Uuid,
    pub wallet_address: String,
    pub password_hash: String,
    pub private_key: String,
    pub profile: DbActorProfile,
}

#[derive(Deserialize)]
pub struct UserCreateData {
    pub username: String,
    pub password: String,
    pub wallet_address: String,
    pub invite_code: Option<String>,
}

fn validate_username(username: &str) -> Result<(), ValidationError> {
    let username_regexp = Regex::new(r"^[a-z0-9_]+$").unwrap();
    if !username_regexp.is_match(username) {
        return Err(ValidationError("invalid username"));
    }
    Ok(())
}

impl UserCreateData {
    /// Validate and clean.
    pub fn clean(&self) -> Result<(), ValidationError> {
        validate_username(&self.username)?;
        Ok(())
    }
}

#[derive(Deserialize)]
pub struct UserLoginData {
    pub signature: String,
    pub wallet_address: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_username() {
        let result_1 = validate_username("name_1");
        assert_eq!(result_1.is_ok(), true);
        let result_2 = validate_username("name&");
        assert_eq!(result_2.is_ok(), false);
    }
}
