use uuid::Uuid;

use mitra_utils::{
    currencies::Currency,
    did::Did,
    did_pkh::DidPkh,
};

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
};
use crate::models::{
    profiles::queries::create_profile,
    profiles::types::{DbActorProfile, ProfileCreateData},
};
use super::types::{
    DbInviteCode,
    DbUser,
    Role,
    User,
    UserCreateData,
};
use super::utils::generate_invite_code;

pub async fn create_invite_code(
    db_client: &impl DatabaseClient,
    note: Option<&str>,
) -> Result<String, DatabaseError> {
    let invite_code = generate_invite_code();
    db_client.execute(
        "
        INSERT INTO user_invite_code (code, note)
        VALUES ($1, $2)
        ",
        &[&invite_code, &note],
    ).await?;
    Ok(invite_code)
}

pub async fn get_invite_codes(
    db_client: &impl DatabaseClient,
) -> Result<Vec<DbInviteCode>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT user_invite_code
        FROM user_invite_code
        WHERE used = FALSE
        ",
        &[],
    ).await?;
    let codes = rows.iter()
        .map(|row| row.try_get("user_invite_code"))
        .collect::<Result<_, _>>()?;
    Ok(codes)
}

pub async fn is_valid_invite_code(
    db_client: &impl DatabaseClient,
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
    db_client: &mut impl DatabaseClient,
    user_data: UserCreateData,
) -> Result<User, DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    // Prevent changes to actor_profile table
    transaction.execute(
        "LOCK TABLE actor_profile IN EXCLUSIVE MODE",
        &[],
    ).await?;
    // Ensure there are no local accounts with a similar name
    let maybe_row = transaction.query_opt(
        "
        SELECT 1
        FROM user_account JOIN actor_profile USING (id)
        WHERE actor_profile.username ILIKE $1
        ",
        &[&user_data.username],
    ).await?;
    if maybe_row.is_some() {
        return Err(DatabaseError::AlreadyExists("user"));
    };
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
        };
    };
    // Create profile
    let profile_data = ProfileCreateData {
        username: user_data.username.clone(),
        hostname: None,
        display_name: None,
        bio: None,
        avatar: None,
        banner: None,
        manually_approves_followers: false,
        identity_proofs: vec![],
        payment_options: vec![],
        extra_fields: vec![],
        aliases: vec![],
        emojis: vec![],
        actor_json: None,
    };
    let profile = create_profile(&mut transaction, profile_data).await?;
    // Create user
    let row = transaction.query_one(
        "
        INSERT INTO user_account (
            id,
            wallet_address,
            password_hash,
            private_key,
            invite_code,
            user_role
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING user_account
        ",
        &[
            &profile.id,
            &user_data.wallet_address,
            &user_data.password_hash,
            &user_data.private_key_pem,
            &user_data.invite_code,
            &user_data.role,
        ],
    ).await.map_err(catch_unique_violation("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let user = User::new(db_user, profile);
    transaction.commit().await?;
    Ok(user)
}

pub async fn set_user_password(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
    password_hash: String,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE user_account SET password_hash = $1
        WHERE id = $2
        ",
        &[&password_hash, &user_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("user"));
    };
    Ok(())
}

pub async fn set_user_role(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
    role: Role,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE user_account SET user_role = $1
        WHERE id = $2
        ",
        &[&role, &user_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("user"));
    };
    Ok(())
}

pub async fn get_user_by_id(
    db_client: &impl DatabaseClient,
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
    db_client: &impl DatabaseClient,
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
    db_client: &impl DatabaseClient,
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

pub async fn get_user_by_login_address(
    db_client: &impl DatabaseClient,
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

pub async fn get_user_by_did(
    db_client: &impl DatabaseClient,
    did: &Did,
) -> Result<User, DatabaseError> {
    // DIDs must be locally unique
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account, actor_profile
        FROM user_account JOIN actor_profile USING (id)
        WHERE
            EXISTS (
                SELECT 1
                FROM jsonb_array_elements(actor_profile.identity_proofs) AS proof
                WHERE proof ->> 'issuer' = $1
            )
        ",
        &[&did.to_string()],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("user"))?;
    let db_user: DbUser = row.try_get("user_account")?;
    let db_profile: DbActorProfile = row.try_get("actor_profile")?;
    let user = User::new(db_user, db_profile);
    Ok(user)
}

pub async fn get_user_by_public_wallet_address(
    db_client: &impl DatabaseClient,
    currency: &Currency,
    wallet_address: &str,
) -> Result<User, DatabaseError> {
    let did_pkh = DidPkh::from_address(currency, wallet_address);
    let did = Did::Pkh(did_pkh);
    get_user_by_did(db_client, &did).await
}

pub async fn get_user_count(
    db_client: &impl DatabaseClient,
) -> Result<i64, DatabaseError> {
    let row = db_client.query_one(
        "SELECT count(user_account) FROM user_account",
        &[],
    ).await?;
    let count = row.try_get("count")?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::models::users::types::Role;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_invite_code() {
        let db_client = &mut create_test_database().await;
        let code = create_invite_code(db_client, Some("test")).await.unwrap();
        assert_eq!(code.len(), 32);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_user() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "myname".to_string(),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        assert_eq!(user.profile.username, "myname");
        assert_eq!(user.role, Role::NormalUser);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_user_impersonation_protection() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData {
            username: "myname".to_string(),
            ..Default::default()
        };
        create_user(db_client, user_data).await.unwrap();
        let another_user_data = UserCreateData {
            username: "myName".to_string(),
            ..Default::default()
        };
        let result = create_user(db_client, another_user_data).await;
        assert!(matches!(result, Err(DatabaseError::AlreadyExists("user"))));
    }

    #[tokio::test]
    #[serial]
    async fn test_set_user_role() {
        let db_client = &mut create_test_database().await;
        let user_data = UserCreateData::default();
        let user = create_user(db_client, user_data).await.unwrap();
        assert_eq!(user.role, Role::NormalUser);
        set_user_role(db_client, &user.id, Role::ReadOnlyUser).await.unwrap();
        let user = get_user_by_id(db_client, &user.id).await.unwrap();
        assert_eq!(user.role, Role::ReadOnlyUser);
    }
}
