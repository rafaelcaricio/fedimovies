use chrono::{DateTime, Utc};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::{DbUser, User};

pub async fn save_oauth_token(
    db_client: &impl GenericClient,
    owner_id: &Uuid,
    access_token: &str,
    created_at: &DateTime<Utc>,
    expires_at: &DateTime<Utc>,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO oauth_token (owner_id, token, created_at, expires_at)
        VALUES ($1, $2, $3, $4)
        ",
        &[&owner_id, &access_token, &created_at, &expires_at],
    ).await?;
    Ok(())
}

pub async fn get_user_by_oauth_token(
    db_client: &impl GenericClient,
    access_token: &str,
) -> Result<User, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM oauth_token
        JOIN user_account ON oauth_token.owner_id = user_account.id
        JOIN actor_profile ON user_account.id = actor_profile.id
        WHERE
            oauth_token.token = $1
            AND oauth_token.expires_at > now()
        ",
        &[&access_token],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile);
    Ok(user)
}
