use chrono::{DateTime, Utc};
use uuid::Uuid;

use mitra_utils::{
    currencies::Currency,
    did::Did,
    did_pkh::DidPkh,
    id::generate_ulid,
};

use crate::cleanup::{
    find_orphaned_files,
    find_orphaned_ipfs_objects,
    DeletionQueue,
};
use crate::database::{
    catch_unique_violation,
    query_macro::query,
    DatabaseClient,
    DatabaseError,
};
use crate::emojis::types::DbEmoji;
use crate::instances::queries::create_instance;
use crate::relationships::types::RelationshipType;

use super::types::{
    Aliases,
    DbActorProfile,
    ExtraFields,
    IdentityProofs,
    PaymentOptions,
    ProfileCreateData,
    ProfileUpdateData,
};

async fn create_profile_emojis(
    db_client: &impl DatabaseClient,
    profile_id: &Uuid,
    emojis: Vec<Uuid>,
) -> Result<Vec<DbEmoji>, DatabaseError> {
    let emojis_rows = db_client.query(
        "
        INSERT INTO profile_emoji (profile_id, emoji_id)
        SELECT $1, emoji.id FROM emoji WHERE id = ANY($2)
        RETURNING (
            SELECT emoji FROM emoji
            WHERE emoji.id = emoji_id
        )
        ",
        &[&profile_id, &emojis],
    ).await?;
    if emojis_rows.len() != emojis.len() {
        return Err(DatabaseError::NotFound("emoji"));
    };
    let emojis = emojis_rows.iter()
        .map(|row| row.try_get("emoji"))
        .collect::<Result<_, _>>()?;
    Ok(emojis)
}

async fn update_emoji_cache(
    db_client: &impl DatabaseClient,
    profile_id: &Uuid,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        WITH profile_emojis AS (
            SELECT
                actor_profile.id AS profile_id,
                COALESCE(
                    jsonb_agg(emoji) FILTER (WHERE emoji.id IS NOT NULL),
                    '[]'
                ) AS emojis
            FROM actor_profile
            LEFT JOIN profile_emoji ON (profile_emoji.profile_id = actor_profile.id)
            LEFT JOIN emoji ON (emoji.id = profile_emoji.emoji_id)
            WHERE actor_profile.id = $1
            GROUP BY actor_profile.id
        )
        UPDATE actor_profile
        SET emojis = profile_emojis.emojis
        FROM profile_emojis
        WHERE actor_profile.id = profile_emojis.profile_id
        RETURNING actor_profile
        ",
        &[&profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile: DbActorProfile = row.try_get("actor_profile")?;
    Ok(profile)
}

pub async fn update_emoji_caches(
    db_client: &impl DatabaseClient,
    emoji_id: &Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        WITH profile_emojis AS (
            SELECT
                actor_profile.id AS profile_id,
                COALESCE(
                    jsonb_agg(emoji) FILTER (WHERE emoji.id IS NOT NULL),
                    '[]'
                ) AS emojis
            FROM actor_profile
            CROSS JOIN jsonb_array_elements(actor_profile.emojis) AS cached_emoji
            LEFT JOIN profile_emoji ON (profile_emoji.profile_id = actor_profile.id)
            LEFT JOIN emoji ON (emoji.id = profile_emoji.emoji_id)
            WHERE CAST(cached_emoji ->> 'id' AS UUID) = $1
            GROUP BY actor_profile.id
        )
        UPDATE actor_profile
        SET emojis = profile_emojis.emojis
        FROM profile_emojis
        WHERE actor_profile.id = profile_emojis.profile_id
        ",
        &[&emoji_id],
    ).await?;
    Ok(())
}

/// Create new profile using given Client or Transaction.
pub async fn create_profile(
    db_client: &mut impl DatabaseClient,
    profile_data: ProfileCreateData,
) -> Result<DbActorProfile, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let profile_id = generate_ulid();
    if let Some(ref hostname) = profile_data.hostname {
        create_instance(&transaction, hostname).await?;
    };
    transaction.execute(
        "
        INSERT INTO actor_profile (
            id,
            username,
            hostname,
            display_name,
            bio,
            bio_source,
            avatar,
            banner,
            manually_approves_followers,
            identity_proofs,
            payment_options,
            extra_fields,
            aliases,
            actor_json
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        RETURNING actor_profile
        ",
        &[
            &profile_id,
            &profile_data.username,
            &profile_data.hostname,
            &profile_data.display_name,
            &profile_data.bio,
            &profile_data.bio,
            &profile_data.avatar,
            &profile_data.banner,
            &profile_data.manually_approves_followers,
            &IdentityProofs(profile_data.identity_proofs),
            &PaymentOptions(profile_data.payment_options),
            &ExtraFields(profile_data.extra_fields),
            &Aliases::new(profile_data.aliases),
            &profile_data.actor_json,
        ],
    ).await.map_err(catch_unique_violation("profile"))?;

    // Create related objects
    create_profile_emojis(
        &transaction,
        &profile_id,
        profile_data.emojis,
    ).await?;
    let profile = update_emoji_cache(&transaction, &profile_id).await?;

    transaction.commit().await?;
    Ok(profile)
}

pub async fn update_profile(
    db_client: &mut impl DatabaseClient,
    profile_id: &Uuid,
    profile_data: ProfileUpdateData,
) -> Result<DbActorProfile, DatabaseError> {
    let transaction = db_client.transaction().await?;
    transaction.execute(
        "
        UPDATE actor_profile
        SET
            display_name = $1,
            bio = $2,
            bio_source = $3,
            avatar = $4,
            banner = $5,
            manually_approves_followers = $6,
            identity_proofs = $7,
            payment_options = $8,
            extra_fields = $9,
            aliases = $10,
            actor_json = $11,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $12
        RETURNING actor_profile
        ",
        &[
            &profile_data.display_name,
            &profile_data.bio,
            &profile_data.bio_source,
            &profile_data.avatar,
            &profile_data.banner,
            &profile_data.manually_approves_followers,
            &IdentityProofs(profile_data.identity_proofs),
            &PaymentOptions(profile_data.payment_options),
            &ExtraFields(profile_data.extra_fields),
            &Aliases::new(profile_data.aliases),
            &profile_data.actor_json,
            &profile_id,
        ],
    ).await?;

    // Delete and re-create related objects
    transaction.execute(
        "DELETE FROM profile_emoji WHERE profile_id = $1",
        &[profile_id],
    ).await?;
    create_profile_emojis(
        &transaction,
        profile_id,
        profile_data.emojis,
    ).await?;
    let profile = update_emoji_cache(&transaction, profile_id).await?;

    transaction.commit().await?;
    Ok(profile)
}

pub async fn get_profile_by_id(
    db_client: &impl DatabaseClient,
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

pub async fn get_profile_by_remote_actor_id(
    db_client: &impl DatabaseClient,
    actor_id: &str,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE actor_id = $1
        ",
        &[&actor_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile: DbActorProfile = row.try_get("actor_profile")?;
    profile.check_remote()?;
    Ok(profile)
}

pub async fn get_profile_by_acct(
    db_client: &impl DatabaseClient,
    acct: &str,
) -> Result<DbActorProfile, DatabaseError> {
    let result = db_client.query_opt(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE actor_profile.acct = $1
        ",
        &[&acct],
    ).await?;
    let profile = match result {
        Some(row) => row.try_get("actor_profile")?,
        None => return Err(DatabaseError::NotFound("profile")),
    };
    Ok(profile)
}

pub async fn get_profiles(
    db_client: &impl DatabaseClient,
    only_local: bool,
    offset: u16,
    limit: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let condition = if only_local { "WHERE actor_id IS NULL" } else { "" };
    let statement = format!(
        "
        SELECT actor_profile
        FROM actor_profile
        {condition}
        ORDER BY username
        LIMIT $1 OFFSET $2
        ",
        condition=condition,
    );
    let rows = db_client.query(
        &statement,
        &[&i64::from(limit), &i64::from(offset)],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn get_profiles_by_accts(
    db_client: &impl DatabaseClient,
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
    db_client: &mut impl DatabaseClient,
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
        SELECT unnest(array_remove(
            ARRAY[
                avatar ->> 'file_name',
                banner ->> 'file_name'
            ],
            NULL
        )) AS file_name
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
        UPDATE actor_profile
        SET subscriber_count = subscriber_count - 1
        FROM relationship
        WHERE
            relationship.source_id = $1
            AND relationship.target_id = actor_profile.id
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Subscription],
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

pub async fn search_profiles(
    db_client: &impl DatabaseClient,
    username: &str,
    maybe_hostname: Option<&String>,
    limit: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let db_search_query = match maybe_hostname {
        Some(hostname) => {
            // Search for exact actor address
            format!("{}@{}", username, hostname)
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
        LIMIT $2
        ",
        &[&db_search_query, &i64::from(limit)],
    ).await?;
    let profiles: Vec<DbActorProfile> = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn search_profiles_by_did_only(
    db_client: &impl DatabaseClient,
    did: &Did,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
     let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE
            EXISTS (
                SELECT 1
                FROM jsonb_array_elements(actor_profile.identity_proofs) AS proof
                WHERE proof ->> 'issuer' = $1
            )
        ",
        &[&did.to_string()],
    ).await?;
    let profiles: Vec<DbActorProfile> = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn search_profiles_by_did(
    db_client: &impl DatabaseClient,
    did: &Did,
    prefer_verified: bool,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let verified = search_profiles_by_did_only(db_client, did).await?;
    let maybe_currency_address = match did {
        Did::Pkh(did_pkh) => {
            did_pkh.currency()
                .map(|currency| (currency, did_pkh.address.clone()))
        },
        _ => None,
    };
    let unverified = if let Some((currency, address)) = maybe_currency_address {
        // If currency is Ethereum,
        // search over extra fields must be case insensitive.
        let value_op = match currency {
            Currency::Ethereum => "ILIKE",
            Currency::Monero => "LIKE",
        };
        // This query does not scan user_account.wallet_address because
        // login addresses are private.
        let statement = format!(
            "
            SELECT actor_profile
            FROM actor_profile
            WHERE
                EXISTS (
                    SELECT 1
                    FROM jsonb_array_elements(actor_profile.extra_fields) AS field
                    WHERE
                        field ->> 'name' ILIKE $field_name
                        AND field ->> 'value' {value_op} $field_value
                )
            ",
            value_op=value_op,
        );
        let field_name = currency.field_name();
        let query = query!(
            &statement,
            field_name=field_name,
            field_value=address,
        )?;
        let rows = db_client.query(query.sql(), query.parameters()).await?;
        let unverified = rows.iter()
            .map(|row| row.try_get("actor_profile"))
            .collect::<Result<Vec<DbActorProfile>, _>>()?
            .into_iter()
            // Exclude verified
            .filter(|profile| !verified.iter().any(|item| item.id == profile.id))
            .collect();
        unverified
    } else {
        vec![]
    };
    let results = if prefer_verified && verified.len() > 0 {
        verified
    } else {
        [verified, unverified].concat()
    };
    Ok(results)
}

pub async fn search_profiles_by_wallet_address(
    db_client: &impl DatabaseClient,
    currency: &Currency,
    wallet_address: &str,
    prefer_verified: bool,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let did_pkh = DidPkh::from_address(currency, wallet_address);
    let did = Did::Pkh(did_pkh);
    search_profiles_by_did(db_client, &did, prefer_verified).await
}

pub async fn update_follower_count(
    db_client: &impl DatabaseClient,
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
    db_client: &impl DatabaseClient,
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

pub async fn update_subscriber_count(
    db_client: &impl DatabaseClient,
    profile_id: &Uuid,
    change: i32,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE actor_profile
        SET subscriber_count = subscriber_count + $1
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
    db_client: &impl DatabaseClient,
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

// Doesn't return error if profile doesn't exist
pub async fn set_reachability_status(
    db_client: &impl DatabaseClient,
    actor_id: &str,
    is_reachable: bool,
) -> Result<(), DatabaseError> {
    if !is_reachable {
        // Don't update profile if unreachable_since is already set
        db_client.execute(
            "
            UPDATE actor_profile
            SET unreachable_since = CURRENT_TIMESTAMP
            WHERE actor_id = $1 AND unreachable_since IS NULL
            ",
            &[&actor_id],
        ).await?;
    } else {
        // Remove status (if set)
        db_client.execute(
            "
            UPDATE actor_profile
            SET unreachable_since = NULL
            WHERE actor_id = $1
            ",
            &[&actor_id],
        ).await?;
    };
    Ok(())
}

pub async fn find_unreachable(
    db_client: &impl DatabaseClient,
    unreachable_since: &DateTime<Utc>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE unreachable_since < $1
        ORDER BY hostname, username
        ",
        &[&unreachable_since],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

/// Finds all empty remote profiles
/// (without any posts, reactions, relationships)
/// updated before the specified date
pub async fn find_empty_profiles(
    db_client: &impl DatabaseClient,
    updated_before: &DateTime<Utc>,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile.id
        FROM actor_profile
        WHERE
            actor_profile.hostname IS NOT NULL
            AND actor_profile.updated_at < $1
            AND NOT EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    source_id = actor_profile.id
                    OR target_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM follow_request
                WHERE
                    source_id = actor_profile.id
                    OR target_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM post
                WHERE author_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM post_reaction
                WHERE author_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM media_attachment
                WHERE owner_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM mention
                WHERE profile_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM notification
                WHERE sender_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM invoice
                WHERE sender_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM subscription
                WHERE sender_id = actor_profile.id
            )
        ",
        &[&updated_before],
    ).await?;
    let ids: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::emojis::{
        queries::create_emoji,
        types::EmojiImage,
    };
    use crate::profiles::{
        queries::create_profile,
        types::{
            DbActor,
            ExtraField,
            IdentityProof,
            IdentityProofType,
            ProfileCreateData,
        },
    };
    use crate::users::{
        queries::create_user,
        types::UserCreateData,
    };
    use super::*;

    fn create_test_actor(actor_id: &str) -> DbActor {
        DbActor { id: actor_id.to_string(), ..Default::default() }
    }

    #[tokio::test]
    #[serial]
    async fn test_create_profile_local() {
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let db_client = &mut create_test_database().await;
        let profile = create_profile(db_client, profile_data).await.unwrap();
        assert_eq!(profile.username, "test");
        assert_eq!(profile.hostname, None);
        assert_eq!(profile.acct, "test");
        assert_eq!(profile.identity_proofs.into_inner().len(), 0);
        assert_eq!(profile.extra_fields.into_inner().len(), 0);
        assert_eq!(profile.actor_id, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_profile_remote() {
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            hostname: Some("example.com".to_string()),
            actor_json: Some(create_test_actor("https://example.com/users/test")),
            ..Default::default()
        };
        let db_client = &mut create_test_database().await;
        let profile = create_profile(db_client, profile_data).await.unwrap();
        assert_eq!(profile.username, "test");
        assert_eq!(profile.hostname.unwrap(), "example.com");
        assert_eq!(profile.acct, "test@example.com");
        assert_eq!(
            profile.actor_id.unwrap(),
            "https://example.com/users/test",
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_create_profile_with_emoji() {
        let db_client = &mut create_test_database().await;
        let image = EmojiImage::default();
        let emoji = create_emoji(
            db_client,
            "testemoji",
            None,
            image,
            None,
            &Utc::now(),
        ).await.unwrap();
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            emojis: vec![emoji.id.clone()],
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let profile_emojis = profile.emojis.into_inner();
        assert_eq!(profile_emojis.len(), 1);
        assert_eq!(profile_emojis[0].id, emoji.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_actor_id_unique() {
        let db_client = &mut create_test_database().await;
        let actor_id = "https://example.com/users/test";
        let profile_data_1 = ProfileCreateData {
            username: "test-1".to_string(),
            hostname: Some("example.com".to_string()),
            actor_json: Some(create_test_actor(actor_id)),
            ..Default::default()
        };
        create_profile(db_client, profile_data_1).await.unwrap();
        let profile_data_2 = ProfileCreateData {
            username: "test-2".to_string(),
            hostname: Some("example.com".to_string()),
            actor_json: Some(create_test_actor(actor_id)),
            ..Default::default()
        };
        let error = create_profile(db_client, profile_data_2).await.err().unwrap();
        assert_eq!(error.to_string(), "profile already exists");
    }

    #[tokio::test]
    #[serial]
    async fn test_update_profile() {
        let db_client = &mut create_test_database().await;
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let mut profile_data = ProfileUpdateData::from(&profile);
        let bio = "test bio";
        profile_data.bio = Some(bio.to_string());
        let profile_updated = update_profile(
            db_client,
            &profile.id,
            profile_data,
        ).await.unwrap();
        assert_eq!(profile_updated.username, profile.username);
        assert_eq!(profile_updated.bio.unwrap(), bio);
        assert!(profile_updated.updated_at != profile.updated_at);
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_profile() {
        let profile_data = ProfileCreateData::default();
        let db_client = &mut create_test_database().await;
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let deletion_queue = delete_profile(db_client, &profile.id).await.unwrap();
        assert_eq!(deletion_queue.files.len(), 0);
        assert_eq!(deletion_queue.ipfs_objects.len(), 0);
    }

    const ETHEREUM: Currency = Currency::Ethereum;

    #[tokio::test]
    #[serial]
    async fn test_search_profiles_by_wallet_address_local() {
        let db_client = &mut create_test_database().await;
        let wallet_address = "0x1234abcd";
        let user_data = UserCreateData {
            wallet_address: Some(wallet_address.to_string()),
            ..Default::default()
        };
        let _user = create_user(db_client, user_data).await.unwrap();
        let profiles = search_profiles_by_wallet_address(
            db_client, &ETHEREUM, wallet_address, false).await.unwrap();

        // Login address must not be exposed
        assert_eq!(profiles.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_search_profiles_by_wallet_address_remote() {
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
        let profiles = search_profiles_by_wallet_address(
            db_client, &ETHEREUM, "0x1234abcd", false).await.unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, profile.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_search_profiles_by_wallet_address_identity_proof() {
        let db_client = &mut create_test_database().await;
        let identity_proof = IdentityProof {
            issuer: Did::Pkh(DidPkh::from_address(&ETHEREUM, "0x1234abcd")),
            proof_type: IdentityProofType::LegacyEip191IdentityProof,
            value: "13590013185bdea963".to_string(),
        };
        let profile_data = ProfileCreateData {
            identity_proofs: vec![identity_proof],
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let profiles = search_profiles_by_wallet_address(
            db_client, &ETHEREUM, "0x1234abcd", false).await.unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, profile.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_set_reachability_status() {
        let db_client = &mut create_test_database().await;
        let actor_id = "https://example.com/users/test";
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            hostname: Some("example.com".to_string()),
            actor_json: Some(create_test_actor(actor_id)),
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        set_reachability_status(db_client, actor_id, false).await.unwrap();
        let profile = get_profile_by_id(db_client, &profile.id).await.unwrap();
        assert_eq!(profile.unreachable_since.is_some(), true);
    }

    #[tokio::test]
    #[serial]
    async fn test_find_empty_profiles() {
        let db_client = &mut create_test_database().await;
        let updated_before = Utc::now();
        let profiles = find_empty_profiles(db_client, &updated_before).await.unwrap();
        assert_eq!(profiles.is_empty(), true);
    }
}
