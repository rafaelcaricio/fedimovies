use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::models::cleanup::{
    find_orphaned_files,
    find_orphaned_ipfs_objects,
    DeletionQueue,
};
use crate::utils::id::new_uuid;
use super::types::{
    ExtraFields,
    DbActorProfile,
    ProfileCreateData,
    ProfileUpdateData,
};

/// Create new profile using given Client or Transaction.
pub async fn create_profile(
    db_client: &impl GenericClient,
    profile_data: ProfileCreateData,
) -> Result<DbActorProfile, DatabaseError> {
    let profile_id = new_uuid();
    let extra_fields = ExtraFields(profile_data.extra_fields.clone());
    let row = db_client.query_one(
        "
        INSERT INTO actor_profile (
            id, username, display_name, acct, bio, bio_source,
            avatar_file_name, banner_file_name, extra_fields,
            actor_json
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
            &extra_fields,
            &profile_data.actor_json,
        ],
    ).await.map_err(catch_unique_violation("profile"))?;
    let profile = row.try_get("actor_profile")?;
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
            banner_file_name = $5,
            extra_fields = $6,
            actor_json = $7
        WHERE id = $8
        RETURNING actor_profile
        ",
        &[
            &data.display_name,
            &data.bio,
            &data.bio_source,
            &data.avatar,
            &data.banner,
            &ExtraFields(data.extra_fields),
            &data.actor_json,
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
    offset: i64,
    limit: i64,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        ORDER BY username
        LIMIT $1 OFFSET $2
        ",
        &[&limit, &offset],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<Vec<DbActorProfile>, _>>()?;
    Ok(profiles)
}

pub async fn get_profiles_by_accts(
    db_client: &impl GenericClient,
    accts: Vec<String>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE acct = ANY($1)
        ",
        &[&accts],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

/// Deletes profile from database and returns collection of orphaned objects.
pub async fn delete_profile(
    db_client: &mut impl GenericClient,
    profile_id: &Uuid,
) -> Result<DeletionQueue, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Get list of media files owned by actor
    let files_rows = transaction.query(
        "
        SELECT file_name
        FROM media_attachment WHERE owner_id = $1
        UNION ALL
        SELECT unnest(array_remove(ARRAY[avatar_file_name, banner_file_name], NULL))
        FROM actor_profile WHERE id = $1
        ",
        &[&profile_id],
    ).await?;
    let files: Vec<String> = files_rows.iter()
        .map(|row| row.try_get("file_name"))
        .collect::<Result<_, _>>()?;
    // Get list of IPFS objects owned by actor
    let ipfs_objects_rows = transaction.query(
        "
        SELECT ipfs_cid
        FROM media_attachment
        WHERE owner_id = $1 AND ipfs_cid IS NOT NULL
        UNION ALL
        SELECT ipfs_cid
        FROM post
        WHERE author_id = $1 AND ipfs_cid IS NOT NULL
        ",
        &[&profile_id],
    ).await?;
    let ipfs_objects: Vec<String> = ipfs_objects_rows.iter()
        .map(|row| row.try_get("ipfs_cid"))
        .collect::<Result<_, _>>()?;
    // Update counters
    transaction.execute(
        "
        UPDATE actor_profile
        SET follower_count = follower_count - 1
        FROM relationship
        WHERE
            actor_profile.id = relationship.target_id
            AND relationship.source_id = $1
        ",
        &[&profile_id],
    ).await?;
    transaction.execute(
        "
        UPDATE actor_profile
        SET following_count = following_count - 1
        FROM relationship
        WHERE
            actor_profile.id = relationship.source_id
            AND relationship.target_id = $1
        ",
        &[&profile_id],
    ).await?;
    transaction.execute(
        "
        UPDATE post
        SET reply_count = reply_count - reply.count
        FROM (
            SELECT in_reply_to_id, count(*) FROM post
            WHERE author_id = $1 AND in_reply_to_id IS NOT NULL
            GROUP BY in_reply_to_id
        ) AS reply
        WHERE post.id = reply.in_reply_to_id
        ",
        &[&profile_id],
    ).await?;
    transaction.execute(
        "
        UPDATE post
        SET reaction_count = reaction_count - 1
        FROM post_reaction
        WHERE
            post_reaction.post_id = post.id
            AND post_reaction.author_id = $1
        ",
        &[&profile_id],
    ).await?;
    transaction.execute(
        "
        UPDATE post
        SET repost_count = post.repost_count - 1
        FROM post AS repost
        WHERE
            repost.repost_of_id = post.id
            AND repost.author_id = $1
        ",
        &[&profile_id],
    ).await?;
    // Delete profile
    let deleted_count = transaction.execute(
        "
        DELETE FROM actor_profile WHERE id = $1
        RETURNING actor_profile
        ",
        &[&profile_id],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("profile"));
    }
    let orphaned_files = find_orphaned_files(&transaction, files).await?;
    let orphaned_ipfs_objects = find_orphaned_ipfs_objects(&transaction, ipfs_objects).await?;
    transaction.commit().await?;
    Ok(DeletionQueue {
        files: orphaned_files,
        ipfs_objects: orphaned_ipfs_objects,
    })
}

pub async fn search_profile(
    db_client: &impl GenericClient,
    username: &str,
    instance: Option<&String>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let db_search_query = match instance {
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
        WHERE acct ILIKE $1
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


#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_profile() {
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let db_client = create_test_database().await;
        let profile = create_profile(&db_client, profile_data).await.unwrap();
        assert_eq!(profile.username, "test");
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_profile() {
        let profile_data = ProfileCreateData::default();
        let mut db_client = create_test_database().await;
        let profile = create_profile(&db_client, profile_data).await.unwrap();
        let deletion_queue = delete_profile(&mut db_client, &profile.id).await.unwrap();
        assert_eq!(deletion_queue.files.len(), 0);
        assert_eq!(deletion_queue.ipfs_objects.len(), 0);
    }
}
