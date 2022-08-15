use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use regex::Regex;
use uuid::Uuid;

use crate::errors::ValidationError;
use crate::models::profiles::types::DbActorProfile;
use crate::utils::currencies::Currency;

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
    pub wallet_address: Option<String>, // login address
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

    /// Returns wallet address if it is verified
    pub fn public_wallet_address(&self, currency: &Currency) -> Option<String> {
        for proof in self.profile.identity_proofs.clone().into_inner() {
            // Return first match (it's safe if user is local)
            if let Some(ref address_currency) = proof.issuer.currency() {
                if address_currency == currency {
                    return Some(proof.issuer.address);
                };
            };
        };
        None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_public_wallet_address_login_address_not_exposed() {
        let user = User {
            wallet_address: Some("0x1234".to_string()),
            ..Default::default()
        };
        let ethereum = Currency::Ethereum;
        assert_eq!(user.public_wallet_address(&ethereum), None);
    }

    #[test]
    fn test_validate_local_username() {
        let result_1 = validate_local_username("name_1");
        assert_eq!(result_1.is_ok(), true);
        let result_2 = validate_local_username("name&");
        assert_eq!(result_2.is_ok(), false);
    }
}
