use crate::database::{DatabaseClient, DatabaseError};
use crate::mastodon_api::errors::MastodonError;
use crate::models::{
    oauth::queries::get_user_by_oauth_token,
    users::types::User,
};

pub async fn get_current_user(
    db_client: &impl DatabaseClient,
    token: &str,
) -> Result<User, MastodonError> {
    let user = get_user_by_oauth_token(db_client, token).await.map_err(|err| {
        match err {
            DatabaseError::NotFound(_) => {
                MastodonError::AuthError("access token is invalid")
            },
            _ => MastodonError::InternalError,
        }
    })?;
    Ok(user)
}
