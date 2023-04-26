use fedimovies_models::users::types::Role;

use crate::errors::ValidationError;

pub const ALLOWED_ROLES: [&str; 3] = ["admin", "user", "read_only_user"];

pub fn role_from_str(role_str: &str) -> Result<Role, ValidationError> {
    let role = match role_str {
        "user" => Role::NormalUser,
        "admin" => Role::Admin,
        "read_only_user" => Role::ReadOnlyUser,
        _ => return Err(ValidationError("unknown role".to_string())),
    };
    Ok(role)
}
