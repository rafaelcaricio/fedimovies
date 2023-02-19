use crate::database::{DatabaseClient, DatabaseError};
use crate::errors::HttpError;
use crate::models::{
    oauth::queries::get_user_by_oauth_token,
    users::types::User,
};

pub async fn get_current_user(
    db_client: &impl DatabaseClient,
    token: &str,
) -> Result<User, HttpError> {
    let user = get_user_by_oauth_token(db_client, token).await.map_err(|err| {
        match err {
            DatabaseError::NotFound(_) => {
                HttpError::AuthError("access token is invalid")
            },
            _ => HttpError::InternalError,
        }
    })?;
    Ok(user)
}
