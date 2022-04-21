use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::models::cleanup::{
    find_orphaned_files,
    find_orphaned_ipfs_objects,
    DeletionQueue,
};
use crate::models::relationships::types::RelationshipType;
use crate::utils::id::new_uuid;
use super::types::{
    get_currency_field_name,
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
        WHERE actor_id = $1
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
    // Select all posts authored by given actor,
    // their descendants and reposts.
    let posts_rows = transaction.query(
        "
        WITH RECURSIVE context (post_id) AS (
            SELECT post.id FROM post
            WHERE post.author_id = $1
            UNION
            SELECT post.id FROM post
            JOIN context ON (
                post.in_reply_to_id = context.post_id
                OR post.repost_of_id = context.post_id
            )
        )
        SELECT post_id FROM context
        ",
        &[&profile_id],
    ).await?;
    let posts: Vec<Uuid> = posts_rows.iter()
        .map(|row| row.try_get("post_id"))
        .collect::<Result<_, _>>()?;
    // Get list of media files
    let files_rows = transaction.query(
        "
        SELECT unnest(array_remove(ARRAY[avatar_file_name, banner_file_name], NULL)) AS file_name
        FROM actor_profile WHERE id = $1
        UNION ALL
        SELECT file_name
        FROM media_attachment WHERE post_id = ANY($2)
        ",
        &[&profile_id, &posts],
    ).await?;
    let files: Vec<String> = files_rows.iter()
        .map(|row| row.try_get("file_name"))
        .collect::<Result<_, _>>()?;
    // Get list of IPFS objects
    let ipfs_objects_rows = transaction.query(
        "
        SELECT ipfs_cid
        FROM media_attachment
        WHERE post_id = ANY($1) AND ipfs_cid IS NOT NULL
        UNION ALL
        SELECT ipfs_cid
        FROM post
        WHERE id = ANY($1) AND ipfs_cid IS NOT NULL
        ",
        &[&posts],
    ).await?;
    let ipfs_objects: Vec<String> = ipfs_objects_rows.iter()
        .map(|row| row.try_get("ipfs_cid"))
        .collect::<Result<_, _>>()?;
    // Update post counters
    transaction.execute(
        "
        UPDATE actor_profile
        SET post_count = post_count - post.count
        FROM (
            SELECT post.author_id, count(*) FROM post
            WHERE post.id = ANY($1)
            GROUP BY post.author_id
        ) AS post
        WHERE actor_profile.id = post.author_id
        ",
        &[&posts],
    ).await?;
    // Update counters
    transaction.execute(
        "
        UPDATE actor_profile
        SET follower_count = follower_count - 1
        FROM relationship
        WHERE
            relationship.source_id = $1
            AND relationship.target_id = actor_profile.id
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Follow],
    ).await?;
    transaction.execute(
        "
        UPDATE actor_profile
        SET following_count = following_count - 1
        FROM relationship
        WHERE
            relationship.source_id = actor_profile.id
            AND relationship.target_id = $1
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Follow],
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
            // Search for exact actor address
            format!("{}@{}", username, instance)
        },
        None => {
            // Fuzzy search for username
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

pub async fn search_profile_by_wallet_address(
    db_client: &impl GenericClient,
    currency_code: &str,
    wallet_address: &str,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let field_name = get_currency_field_name(currency_code);
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile LEFT JOIN user_account USING (id)
        WHERE
            user_account.wallet_address ILIKE $2
            OR EXISTS (
                SELECT 1
                FROM jsonb_array_elements(actor_profile.extra_fields) AS field
                WHERE
                    field ->> 'name' ILIKE $1
                    AND field ->> 'value' ILIKE $2
            )
        ",
        &[&field_name, &wallet_address],
    ).await?;
    let profiles: Vec<DbActorProfile> = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

/// Get wallet address corresponding to local profile
pub async fn get_wallet_address(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
) -> Result<Option<String>, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT user_account.wallet_address
        FROM actor_profile
        LEFT JOIN user_account ON (actor_profile.id = user_account.id)
        WHERE actor_profile.id = $1
        ",
        &[&profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let wallet_address = row.try_get("wallet_address")?;
    Ok(wallet_address)
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
    use crate::activitypub::actor::Actor;
    use crate::database::test_utils::create_test_database;
    use crate::models::profiles::queries::create_profile;
    use crate::models::profiles::types::{ExtraField, ProfileCreateData};
    use crate::models::users::queries::create_user;
    use crate::models::users::types::UserCreateData;
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
    async fn test_actor_id_unique() {
        let db_client = create_test_database().await;
        let actor_id = "https://example.com/users/test";
        let create_actor = |actor_id: &str| {
            Actor { id: actor_id.to_string(), ..Default::default() }
        };
        let profile_data_1 = ProfileCreateData {
            username: "test-1".to_string(),
            acct: "test-1@example.com".to_string(),
            actor_json: Some(create_actor(actor_id)),
            ..Default::default()
        };
        create_profile(&db_client, profile_data_1).await.unwrap();
        let profile_data_2 = ProfileCreateData {
            username: "test-2".to_string(),
            acct: "test-2@example.com".to_string(),
            actor_json: Some(create_actor(actor_id)),
            ..Default::default()
        };
        let error = create_profile(&db_client, profile_data_2).await.err().unwrap();
        assert_eq!(error.to_string(), "profile already exists");
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

    #[tokio::test]
    #[serial]
    async fn test_search_profile_by_wallet_address_local() {
        let db_client = &mut create_test_database().await;
        let wallet_address = "0x1234abcd";
        let user_data = UserCreateData {
            wallet_address: Some(wallet_address.to_string()),
            ..Default::default()
        };
        let user = create_user(db_client, user_data).await.unwrap();
        let profiles = search_profile_by_wallet_address(
            db_client, "ETH", wallet_address).await.unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, user.profile.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_search_profile_by_wallet_address_remote() {
        let db_client = &mut create_test_database().await;
        let extra_field = ExtraField {
            name: "$eth".to_string(),
            value: "0x1234aBcD".to_string(),
            value_source: None,
        };
        let profile_data = ProfileCreateData {
            extra_fields: vec![extra_field],
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let profiles = search_profile_by_wallet_address(
            db_client, "ETH", "0x1234abcd").await.unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, profile.id);
    }
}
