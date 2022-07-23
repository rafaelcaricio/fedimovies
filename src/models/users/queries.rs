use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::models::profiles::queries::create_profile;
use crate::models::profiles::types::{DbActorProfile, ProfileCreateData};
use super::types::{DbUser, User, UserCreateData};
use super::utils::generate_invite_code;

pub async fn create_invite_code(
    db_client: &impl GenericClient,
) -> Result<String, DatabaseError> {
    let invite_code = generate_invite_code();
    db_client.execute(
        "
        INSERT INTO user_invite_code (code)
        VALUES ($1)
        ",
        &[&invite_code],
    ).await?;
    Ok(invite_code)
}

pub async fn get_invite_codes(
    db_client: &impl GenericClient,
) -> Result<Vec<String>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT code
        FROM user_invite_code
        WHERE used = FALSE
        ",
        &[],
    ).await?;
    let codes: Vec<String> = rows.iter()
        .map(|row| row.try_get("code"))
        .collect::<Result<_, _>>()?;
    Ok(codes)
}

pub async fn is_valid_invite_code(
    db_client: &impl GenericClient,
    invite_code: &str,
) -> Result<bool, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT 1 FROM user_invite_code
        WHERE code = $1 AND used = FALSE
        ",
        &[&invite_code],
    ).await?;
    Ok(maybe_row.is_some())
}

pub async fn create_user(
    db_client: &mut impl GenericClient,
    user_data: UserCreateData,
) -> Result<User, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Use invite code
    if let Some(ref invite_code) = user_data.invite_code {
        let updated_count = transaction.execute(
            "
            UPDATE user_invite_code
            SET used = TRUE
            WHERE code = $1 AND used = FALSE
            ",
            &[&invite_code],
        ).await?;
        if updated_count == 0 {
            return Err(DatabaseError::NotFound("invite code"));
        }
    }
    // Create profile
    let profile_data = ProfileCreateData {
        username: user_data.username.clone(),
        display_name: None,
        acct: user_data.username.clone(),
        bio: None,
        avatar: None,
        banner: None,
        identity_proofs: vec![],
        payment_options: vec![],
        extra_fields: vec![],
        actor_json: None,
    };
    let profile = create_profile(&transaction, profile_data).await?;
    // Create user
    let row = transaction.query_one(
        "
        INSERT INTO user_account (
            id, wallet_address, password_hash, private_key, invite_code
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING user_account
        ",
        &[
            &profile.id,
            &user_data.wallet_address,
            &user_data.password_hash,
            &user_data.private_key_pem,
            &user_data.invite_code,
        ],
    ).await.map_err(catch_unique_violation("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let user = User::new(db_user, profile);
    transaction.commit().await?;
    Ok(user)
}

pub async fn get_user_by_id(
    db_client: &impl GenericClient,
    user_id: &Uuid,
) -> Result<User, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE id = $1
        ",
        &[&user_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile);
    Ok(user)
}

pub async fn get_user_by_name(
    db_client: &impl GenericClient,
    username: &str,
) -> Result<User, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE actor_profile.username = $1
        ",
        &[&username],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile);
    Ok(user)
}

pub async fn is_registered_user(
    db_client: &impl GenericClient,
    username: &str,
) -> Result<bool, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT 1 FROM user_account JOIN actor_profile USING (id)
        WHERE actor_profile.username = $1
        ",
        &[&username],
    ).await?;
    Ok(maybe_row.is_some())
}

pub async fn get_user_by_wallet_address(
    db_client: &impl GenericClient,
    wallet_address: &str,
) -> Result<User, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE wallet_address = $1
        ",
        &[&wallet_address],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile);
    Ok(user)
}

pub async fn get_user_count(
    db_client: &impl GenericClient,
) -> Result<i64, DatabaseError> {
    let row = db_client.query_one(
        "SELECT count(user_account) FROM user_account",
        &[],
    ).await?;
    let count = row.try_get("count")?;
    Ok(count)
}
