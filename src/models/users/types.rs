use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use regex::Regex;
use uuid::Uuid;

use crate::errors::ValidationError;
use crate::models::profiles::types::DbActorProfile;

#[allow(dead_code)]
#[derive(FromSql)]
#[postgres(name = "user_account")]
pub struct DbUser {
    id: Uuid,
    wallet_address: Option<String>,
    password_hash: Option<String>,
    private_key: String,
    invite_code: Option<String>,
    created_at: DateTime<Utc>,
}

// Represents local user
#[derive(Clone)]
#[cfg_attr(test, derive(Default))]
pub struct User {
    pub id: Uuid,
    pub wallet_address: Option<String>,
    pub password_hash: Option<String>,
    pub private_key: String,
    pub profile: DbActorProfile,
}

impl User {
    pub fn new(
        db_user: DbUser,
        db_profile: DbActorProfile,
    ) -> Self {
        assert_eq!(db_user.id, db_profile.id);
        Self {
            id: db_user.id,
            wallet_address: db_user.wallet_address,
            password_hash: db_user.password_hash,
            private_key: db_user.private_key,
            profile: db_profile,
        }
    }

    /// Returns login address if it is verified
    pub fn public_wallet_address(&self) -> Option<String> {
        let wallet_address = self.wallet_address.clone()?;
        let is_verified = self.profile.identity_proofs.clone().into_inner().iter()
            .any(|proof| proof.issuer.address == wallet_address);
        if is_verified { Some(wallet_address) } else { None }
    }
}

#[cfg_attr(test, derive(Default))]
pub struct UserCreateData {
    pub username: String,
    pub password_hash: Option<String>,
    pub private_key_pem: String,
    pub wallet_address: Option<String>,
    pub invite_code: Option<String>,
}

pub fn validate_local_username(username: &str) -> Result<(), ValidationError> {
    // The username regexp should not allow domain names and IP addresses
    let username_regexp = Regex::new(r"^[a-z0-9_]+$").unwrap();
    if !username_regexp.is_match(username) {
        return Err(ValidationError("invalid username"));
    }
    Ok(())
}

/// Verifies that wallet address is valid ethereum address
pub fn validate_wallet_address(wallet_address: &str) -> Result<(), ValidationError> {
    // Address should be lowercase
    let address_regexp = Regex::new(r"^0x[a-f0-9]{40}$").unwrap();
    if !address_regexp.is_match(wallet_address) {
        return Err(ValidationError("address is not lowercase"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_wallet_address_hidden_by_default() {
        let user = User {
            wallet_address: Some("0x1234".to_string()),
            ..Default::default()
        };
        assert_eq!(user.public_wallet_address(), None);
    }

    #[test]
    fn test_validate_local_username() {
        let result_1 = validate_local_username("name_1");
        assert_eq!(result_1.is_ok(), true);
        let result_2 = validate_local_username("name&");
        assert_eq!(result_2.is_ok(), false);
    }

    #[test]
    fn test_validate_wallet_address() {
        let result_1 = validate_wallet_address("0xab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert_eq!(result_1.is_ok(), true);
        let result_2 = validate_wallet_address("ab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert_eq!(result_2.is_ok(), false);
        let result_3 = validate_wallet_address("0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B");
        assert_eq!(result_3.is_ok(), false);
    }
}
