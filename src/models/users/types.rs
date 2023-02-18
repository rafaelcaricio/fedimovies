use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use regex::Regex;
use uuid::Uuid;

use mitra_utils::currencies::Currency;

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseTypeError,
};
use crate::errors::ValidationError;
use crate::identity::did::Did;
use crate::models::profiles::types::DbActorProfile;

#[derive(PartialEq)]
pub enum Permission {
    CreateFollowRequest,
    CreatePost,
    ManageSubscriptionOptions,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Role {
    Guest,
    NormalUser,
    Admin,
    ReadOnlyUser,
}

impl Default for Role {
    fn default() -> Self { Self::NormalUser }
}

impl Role {
    pub fn from_name(name: &str) -> Result<Self, ValidationError> {
        let role = match name {
            "user" => Self::NormalUser,
            "admin" => Self::Admin,
            "read_only_user" => Self::ReadOnlyUser,
            _ => return Err(ValidationError("unknown role")),
        };
        Ok(role)
    }

    pub fn get_permissions(&self) -> Vec<Permission> {
        match self {
            Self::Guest => vec![],
            Self::NormalUser => vec![
                Permission::CreateFollowRequest,
                Permission::CreatePost,
                Permission::ManageSubscriptionOptions,
            ],
            Self::Admin => vec![
                Permission::CreateFollowRequest,
                Permission::CreatePost,
                Permission::ManageSubscriptionOptions,
            ],
            Self::ReadOnlyUser => vec![
                Permission::CreateFollowRequest,
            ],
        }
    }

    pub fn has_permission(&self, permission: Permission) -> bool {
        self.get_permissions().contains(&permission)
    }
}

impl From<&Role> for i16 {
    fn from(value: &Role) -> i16 {
        match value {
            Role::Guest => 0,
            Role::NormalUser => 1,
            Role::Admin => 2,
            Role::ReadOnlyUser => 3,
        }
    }
}

impl TryFrom<i16> for Role {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let role = match value {
            0 => Self::Guest,
            1 => Self::NormalUser,
            2 => Self::Admin,
            3 => Self::ReadOnlyUser,
            _ => return Err(DatabaseTypeError),
        };
        Ok(role)
    }
}

int_enum_from_sql!(Role);
int_enum_to_sql!(Role);

#[allow(dead_code)]
#[derive(FromSql)]
#[postgres(name = "user_account")]
pub struct DbUser {
    id: Uuid,
    wallet_address: Option<String>,
    password_hash: Option<String>,
    private_key: String,
    invite_code: Option<String>,
    user_role: Role,
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
    pub role: Role,
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
            role: db_user.user_role,
            profile: db_profile,
        }
    }

    /// Returns wallet address if it is verified
    pub fn public_wallet_address(&self, currency: &Currency) -> Option<String> {
        for proof in self.profile.identity_proofs.clone().into_inner() {
            let did_pkh = match proof.issuer {
                Did::Pkh(did_pkh) => did_pkh,
                _ => continue,
            };
            // Return the first matching address, because only
            // one proof per currency is allowed.
            if let Some(ref address_currency) = did_pkh.currency() {
                if address_currency == currency {
                    return Some(did_pkh.address);
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
    pub role: Role,
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
