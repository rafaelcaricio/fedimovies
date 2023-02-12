use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
};
use crate::models::{
    profiles::types::DbActorProfile,
    users::types::{DbUser, User},
};
use super::types::{DbOauthApp, DbOauthAppData};

pub async fn create_oauth_app(
    db_client: &impl DatabaseClient,
    app_data: DbOauthAppData,
) -> Result<DbOauthApp, DatabaseError> {
    let row = db_client.query_one(
        "
        INSERT INTO oauth_application (
            app_name,
            website,
            scopes,
            redirect_uri,
            client_id,
            client_secret
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING oauth_application
        ",
        &[
            &app_data.app_name,
            &app_data.website,
            &app_data.scopes,
            &app_data.redirect_uri,
            &app_data.client_id,
            &app_data.client_secret,
        ],
    ).await.map_err(catch_unique_violation("oauth_application"))?;
    let app = row.try_get("oauth_application")?;
    Ok(app)
}

pub async fn get_oauth_app_by_client_id(
    db_client: &impl DatabaseClient,
    client_id: &Uuid,
) -> Result<DbOauthApp, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT oauth_application
        FROM oauth_application
        WHERE client_id = $1
        ",
        &[&client_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("oauth application"))?;
    let app = row.try_get("oauth_application")?;
    Ok(app)
}

pub async fn create_oauth_authorization(
    db_client: &impl DatabaseClient,
    authorization_code: &str,
    user_id: &Uuid,
    application_id: i32,
    scopes: &str,
    created_at: &DateTime<Utc>,
    expires_at: &DateTime<Utc>,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO oauth_authorization (
            code,
            user_id,
            application_id,
            scopes,
            created_at,
            expires_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
        &[
            &authorization_code,
            &user_id,
            &application_id,
            &scopes,
            &created_at,
            &expires_at,
        ],
    ).await?;
    Ok(())
}

pub async fn get_user_by_authorization_code(
    db_client: &impl DatabaseClient,
    authorization_code: &str,
) -> Result<User, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM oauth_authorization
        JOIN user_account ON oauth_authorization.user_id = user_account.id
        JOIN actor_profile ON user_account.id = actor_profile.id
        WHERE
            oauth_authorization.code = $1
            AND oauth_authorization.expires_at > CURRENT_TIMESTAMP
        ",
        &[&authorization_code],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("authorization"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile);
    Ok(user)
}

pub async fn save_oauth_token(
    db_client: &impl DatabaseClient,
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
    db_client: &mut impl DatabaseClient,
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
    db_client: &impl DatabaseClient,
    owner_id: &Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "DELETE FROM oauth_token WHERE owner_id = $1",
        &[&owner_id],
    ).await?;
    Ok(())
}

pub async fn get_user_by_oauth_token(
    db_client: &impl DatabaseClient,
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
    async fn test_create_oauth_app() {
        let db_client = &create_test_database().await;
        let db_app_data = DbOauthAppData {
            app_name: "My App".to_string(),
            ..Default::default()
        };
        let app = create_oauth_app(db_client, db_app_data).await.unwrap();
        assert_eq!(app.app_name, "My App");
    }

    #[tokio::test]
    #[serial]
    async fn test_create_oauth_authorization() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        let app_data = DbOauthAppData {
            app_name: "My App".to_string(),
            ..Default::default()
        };
        let app = create_oauth_app(db_client, app_data).await.unwrap();
        create_oauth_authorization(
            db_client,
            "code",
            &user.id,
            app.id,
            "read write",
            &Utc::now(),
            &Utc::now(),
        ).await.unwrap();
    }

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
