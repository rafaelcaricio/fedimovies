use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use super::types::{DbActorProfile, ProfileCreateData, ProfileUpdateData};

/// Create new profile using given Client or Transaction.
pub async fn create_profile(
    db_client: &impl GenericClient,
    profile_data: &ProfileCreateData,
) -> Result<DbActorProfile, DatabaseError> {
    let profile_id = Uuid::new_v4();
    let result = db_client.query_one(
        "
        INSERT INTO actor_profile (
            id, username, display_name, acct, bio, bio_source,
            avatar_file_name, banner_file_name,
            actor_json
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING actor_profile
        ",
        &[
            &profile_id,
            &profile_data.username,
            &profile_data.display_name,
            &profile_data.acct,
            &profile_data.bio,
            &profile_data.bio,
            &profile_data.avatar,
            &profile_data.banner,
            &profile_data.actor,
        ],
    ).await;
    let profile = match result {
        Ok(row) => row.try_get("actor_profile")?,
        Err(err) => {
            // TODO: catch profile already exists error
            log::warn!("{}", err);
            return Err(DatabaseError::AlreadyExists("profile"));
        },
    };
    Ok(profile)
}

pub async fn update_profile(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
    data: ProfileUpdateData,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE actor_profile
        SET
            display_name = $1,
            bio = $2,
            bio_source = $3,
            avatar_file_name = $4,
            banner_file_name = $5
        WHERE id = $6
        RETURNING actor_profile
        ",
        &[
            &data.display_name,
            &data.bio,
            &data.bio_source,
            &data.avatar,
            &data.banner,
            &profile_id,
        ],
    ).await?;
    let profile = match maybe_row {
        Some(row) => row.try_get("actor_profile")?,
        None => return Err(DatabaseError::NotFound("profile")),
    };
    Ok(profile)
}

pub async fn get_profile_by_id(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
) -> Result<DbActorProfile, DatabaseError> {
    let result = db_client.query_opt(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE id = $1
        ",
        &[&profile_id],
    ).await?;
    let profile = match result {
        Some(row) => row.try_get("actor_profile")?,
        None => return Err(DatabaseError::NotFound("profile")),
    };
    Ok(profile)
}

pub async fn get_profile_by_actor_id(
    db_client: &impl GenericClient,
    actor_id: &str,
) -> Result<DbActorProfile, DatabaseError> {
    let result = db_client.query_opt(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE actor_profile.actor_json ->> 'id' = $1
        ",
        &[&actor_id],
    ).await?;
    let profile = match result {
        Some(row) => row.try_get("actor_profile")?,
        None => return Err(DatabaseError::NotFound("profile")),
    };
    Ok(profile)
}

pub async fn get_profile_by_acct(
    db_client: &impl GenericClient,
    account_uri: &str,
) -> Result<DbActorProfile, DatabaseError> {
    let result = db_client.query_opt(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE actor_profile.acct = $1
        ",
        &[&account_uri],
    ).await?;
    let profile = match result {
        Some(row) => row.try_get("actor_profile")?,
        None => return Err(DatabaseError::NotFound("profile")),
    };
    Ok(profile)
}

pub async fn get_profiles(
    db_client: &impl GenericClient,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        ORDER BY username
        ",
        &[],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<Vec<DbActorProfile>, _>>()?;
    Ok(profiles)
}

pub async fn get_followers(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.source_id)
        WHERE relationship.target_id = $1
        ",
        &[&profile_id],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<Vec<DbActorProfile>, _>>()?;
    Ok(profiles)
}

pub async fn delete_profile(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
) -> Result<(), DatabaseError> {
    let deleted_count = db_client.execute(
        "DELETE FROM actor_profile WHERE id = $1",
        &[&profile_id],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("profile"));
    }
    Ok(())
}

pub async fn search_profile(
    db_client: &impl GenericClient,
    username: &String,
    instance: &Option<String>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let db_search_query = match &instance {
        Some(instance) => {
            // Search for exact profile name.
            // Fetch from remote server if not found
            format!("{}@{}", username, instance)
        },
        None => {
            // Search for username
            format!("%{}%", username)
        },
    };
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE acct LIKE $1
        ",
        &[&db_search_query],
    ).await?;
    let profiles: Vec<DbActorProfile> = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn update_follower_count(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
    change: i32,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE actor_profile
        SET follower_count = follower_count + $1
        WHERE id = $2
        RETURNING actor_profile
        ",
        &[&change, &profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = row.try_get("actor_profile")?;
    Ok(profile)
}

pub async fn update_following_count(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
    change: i32,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE actor_profile
        SET following_count = following_count + $1
        WHERE id = $2
        RETURNING actor_profile
        ",
        &[&change, &profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = row.try_get("actor_profile")?;
    Ok(profile)
}

pub async fn update_post_count(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
    change: i32,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE actor_profile
        SET post_count = post_count + $1
        WHERE id = $2
        RETURNING actor_profile
        ",
        &[&change, &profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = row.try_get("actor_profile")?;
    Ok(profile)
}
