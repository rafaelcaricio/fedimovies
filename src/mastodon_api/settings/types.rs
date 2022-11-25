use serde::Deserialize;

#[derive(Deserialize)]
pub struct PasswordChangeRequest {
    pub new_password: String,
}
