use chrono::{DateTime, Utc};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::{DbUser, User};

pub async fn save_oauth_token(
    db_client: &impl GenericClient,
    owner_id: &Uuid,
    token: &str,
    created_at: &DateTime<Utc>,
    expires_at: &DateTime<Utc>,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO oauth_token (owner_id, token, created_at, expires_at)
        VALUES ($1, $2, $3, $4)
        ",
        &[&owner_id, &token, &created_at, &expires_at],
    ).await?;
    Ok(())
}

pub async fn delete_oauth_token(
    db_client: &mut impl GenericClient,
    current_user_id: &Uuid,
    token: &str,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        SELECT owner_id FROM oauth_token
        WHERE token = $1
        FOR UPDATE
        ",
        &[&token],
    ).await?;
    if let Some(row) = maybe_row {
        let owner_id: Uuid = row.try_get("owner_id")?;
        if owner_id != *current_user_id {
            // Return error if token is owned by a different user
            return Err(DatabaseError::NotFound("token"));
        } else {
            transaction.execute(
                "DELETE FROM oauth_token WHERE token = $1",
                &[&token],
            ).await?;
        };
    };
    transaction.commit().await?;
    Ok(())
}

pub async fn delete_oauth_tokens(
    db_client: &impl GenericClient,
    owner_id: &Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "DELETE FROM oauth_token WHERE owner_id = $1",
        &[&owner_id],
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
            AND oauth_token.expires_at > CURRENT_TIMESTAMP
        ",
        &[&access_token],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile);
    Ok(user)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::models::users::queries::create_user;
    use crate::models::users::types::UserCreateData;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_delete_oauth_token() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        let token = "test-token";
        save_oauth_token(
            db_client,
            &user.id,
            token,
            &Utc::now(),
            &Utc::now(),
        ).await.unwrap();
        delete_oauth_token(
            db_client,
            &user.id,
            token,
        ).await.unwrap();
    }
}
