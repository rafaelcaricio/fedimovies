use serde::Deserialize;

#[derive(Deserialize)]
pub struct PasswordChangeRequest {
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct MoveFollowersRequest {
    pub from_actor_id: String,
    pub followers_csv: String,
}
