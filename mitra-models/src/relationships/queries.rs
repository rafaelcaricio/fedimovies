use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
};
use crate::notifications::queries::create_follow_notification;
use crate::profiles::{
    queries::{
        update_follower_count,
        update_following_count,
        update_subscriber_count,
    },
    types::DbActorProfile,
};

use super::types::{
    DbFollowRequest,
    DbRelationship,
    FollowRequestStatus,
    RelatedActorProfile,
    RelationshipType,
};

pub async fn get_relationships(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<Vec<DbRelationship>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT source_id, target_id, relationship_type
        FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            OR
            source_id = $2 AND target_id = $1
        UNION ALL
        SELECT source_id, target_id, $4
        FROM follow_request
        WHERE
            source_id = $1 AND target_id = $2
            AND request_status = $3
        ",
        &[
            &source_id,
            &target_id,
            &FollowRequestStatus::Pending,
            &RelationshipType::FollowRequest,
        ],
    ).await?;
     let relationships = rows.iter()
        .map(DbRelationship::try_from)
        .collect::<Result<_, _>>()?;
    Ok(relationships)
}

pub async fn has_relationship(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
    relationship_type: RelationshipType,
) -> Result<bool, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT 1
        FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[
            &source_id,
            &target_id,
            &relationship_type,
        ],
    ).await?;
    Ok(maybe_row.is_some())
}

pub async fn follow(
    db_client: &mut impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    transaction.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ",
        &[&source_id, &target_id, &RelationshipType::Follow],
    ).await.map_err(catch_unique_violation("relationship"))?;
    let target_profile = update_follower_count(&transaction, target_id, 1).await?;
    update_following_count(&transaction, source_id, 1).await?;
    if target_profile.is_local() {
        create_follow_notification(&transaction, source_id, target_id).await?;
    };
    transaction.commit().await?;
    Ok(())
}

pub async fn unfollow(
    db_client: &mut impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<Option<Uuid>, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let deleted_count = transaction.execute(
        "
        DELETE FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[&source_id, &target_id, &RelationshipType::Follow],
    ).await?;
    let relationship_deleted = deleted_count > 0;
    // Delete follow request (for remote follows)
    let follow_request_deleted = delete_follow_request_opt(
        &transaction,
        source_id,
        target_id,
    ).await?;
    if !relationship_deleted && follow_request_deleted.is_none() {
        return Err(DatabaseError::NotFound("relationship"));
    };
    if relationship_deleted {
        // Also reset repost and reply visibility settings
        show_reposts(&transaction, source_id, target_id).await?;
        show_replies(&transaction, source_id, target_id).await?;
        // Update counters only if relationship existed
        update_follower_count(&transaction, target_id, -1).await?;
        update_following_count(&transaction, source_id, -1).await?;
    };
    transaction.commit().await?;
    Ok(follow_request_deleted)
}

// Follow remote actor
pub async fn create_follow_request(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<DbFollowRequest, DatabaseError> {
    let request_id = generate_ulid();
    let row = db_client.query_one(
        "
        INSERT INTO follow_request (
            id, source_id, target_id, request_status
        )
        VALUES ($1, $2, $3, $4)
        RETURNING follow_request
        ",
        &[
            &request_id,
            &source_id,
            &target_id,
            &FollowRequestStatus::Pending,
        ],
    ).await.map_err(catch_unique_violation("follow request"))?;
    let request = row.try_get("follow_request")?;
    Ok(request)
}

// Save follow request from remote actor
pub async fn create_remote_follow_request_opt(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
    activity_id: &str,
) -> Result<DbFollowRequest, DatabaseError> {
    let request_id = generate_ulid();
    let row = db_client.query_one(
        "
        INSERT INTO follow_request (
            id,
            source_id,
            target_id,
            activity_id,
            request_status
        )
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (source_id, target_id)
        DO UPDATE SET activity_id = $4
        RETURNING follow_request
        ",
        &[
            &request_id,
            &source_id,
            &target_id,
            &activity_id,
            &FollowRequestStatus::Pending,
        ],
    ).await?;
    let request = row.try_get("follow_request")?;
    Ok(request)
}

pub async fn follow_request_accepted(
    db_client: &mut impl DatabaseClient,
    request_id: &Uuid,
) -> Result<(), DatabaseError> {
    let mut transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        UPDATE follow_request
        SET request_status = $1
        WHERE id = $2
        RETURNING source_id, target_id
        ",
        &[&FollowRequestStatus::Accepted, &request_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("follow request"))?;
    let source_id: Uuid = row.try_get("source_id")?;
    let target_id: Uuid = row.try_get("target_id")?;
    follow(&mut transaction, &source_id, &target_id).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn follow_request_rejected(
    db_client: &impl DatabaseClient,
    request_id: &Uuid,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE follow_request
        SET request_status = $1
        WHERE id = $2
        ",
        &[&FollowRequestStatus::Rejected, &request_id],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("follow request"));
    }
    Ok(())
}

async fn delete_follow_request_opt(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<Option<Uuid>, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        DELETE FROM follow_request
        WHERE source_id = $1 AND target_id = $2
        RETURNING id
        ",
        &[&source_id, &target_id],
    ).await?;
    let maybe_request_id = if let Some(row) = maybe_row {
        let request_id: Uuid = row.try_get("id")?;
        Some(request_id)
    } else { None };
    Ok(maybe_request_id)
}

pub async fn get_follow_request_by_id(
    db_client:  &impl DatabaseClient,
    request_id: &Uuid,
) -> Result<DbFollowRequest, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT follow_request
        FROM follow_request
        WHERE id = $1
        ",
        &[&request_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("follow request"))?;
    let request = row.try_get("follow_request")?;
    Ok(request)
}

pub async fn get_follow_request_by_activity_id(
    db_client: &impl DatabaseClient,
    activity_id: &str,
) -> Result<DbFollowRequest, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT follow_request
        FROM follow_request
        WHERE activity_id = $1
        ",
        &[&activity_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("follow request"))?;
    let request = row.try_get("follow_request")?;
    Ok(request)
}

pub async fn get_followers(
    db_client: &impl DatabaseClient,
    profile_id: &Uuid,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.source_id)
        WHERE
            relationship.target_id = $1
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Follow],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn get_followers_paginated(
    db_client: &impl DatabaseClient,
    profile_id: &Uuid,
    max_relationship_id: Option<i32>,
    limit: u16,
) -> Result<Vec<RelatedActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT relationship.id, actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.source_id)
        WHERE
            relationship.target_id = $1
            AND relationship.relationship_type = $2
            AND ($3::integer IS NULL OR relationship.id < $3)
        ORDER BY relationship.id DESC
        LIMIT $4
        ",
        &[
            &profile_id,
            &RelationshipType::Follow,
            &max_relationship_id,
            &i64::from(limit),
        ],
    ).await?;
    let related_profiles = rows.iter()
        .map(RelatedActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(related_profiles)
}

pub async fn has_local_followers(
    db_client: &impl DatabaseClient,
    actor_id: &str,
) -> Result<bool, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT 1
        FROM relationship
        JOIN actor_profile ON (relationship.target_id = actor_profile.id)
        WHERE
            actor_profile.actor_id = $1
            AND relationship_type = $2
        ",
        &[&actor_id, &RelationshipType::Follow]
    ).await?;
    Ok(maybe_row.is_some())
}

pub async fn get_following(
    db_client: &impl DatabaseClient,
    profile_id: &Uuid,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.target_id)
        WHERE
            relationship.source_id = $1
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Follow],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn get_following_paginated(
    db_client: &impl DatabaseClient,
    profile_id: &Uuid,
    max_relationship_id: Option<i32>,
    limit: u16,
) -> Result<Vec<RelatedActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT relationship.id, actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.target_id)
        WHERE
            relationship.source_id = $1
            AND relationship.relationship_type = $2
            AND ($3::integer IS NULL OR relationship.id < $3)
        ORDER BY relationship.id DESC
        LIMIT $4
        ",
        &[
            &profile_id,
            &RelationshipType::Follow,
            &max_relationship_id,
            &i64::from(limit),
        ],
    ).await?;
    let related_profiles = rows.iter()
        .map(RelatedActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(related_profiles)
}

pub async fn subscribe(
    db_client: &mut impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    transaction.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ",
        &[&source_id, &target_id, &RelationshipType::Subscription],
    ).await.map_err(catch_unique_violation("relationship"))?;
    update_subscriber_count(&transaction, target_id, 1).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn subscribe_opt(
    db_client: &mut impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let inserted_count = transaction.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ON CONFLICT (source_id, target_id, relationship_type) DO NOTHING
        ",
        &[&source_id, &target_id, &RelationshipType::Subscription],
    ).await?;
    if inserted_count > 0 {
        update_subscriber_count(&transaction, target_id, 1).await?;
    };
    transaction.commit().await?;
    Ok(())
}

pub async fn unsubscribe(
    db_client: &mut impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let deleted_count = transaction.execute(
        "
        DELETE FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[&source_id, &target_id, &RelationshipType::Subscription],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("relationship"));
    };
    update_subscriber_count(&transaction, target_id, -1).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn get_subscribers(
    db_client: &impl DatabaseClient,
    profile_id: &Uuid,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        JOIN relationship
        ON (actor_profile.id = relationship.source_id)
        WHERE
            relationship.target_id = $1
            AND relationship.relationship_type = $2
        ORDER BY relationship.id DESC
        ",
        &[&profile_id, &RelationshipType::Subscription],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn hide_reposts(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ON CONFLICT (source_id, target_id, relationship_type) DO NOTHING
        ",
        &[&source_id, &target_id, &RelationshipType::HideReposts],
    ).await?;
    Ok(())
}

pub async fn show_reposts(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    // Does not return NotFound error
    db_client.execute(
        "
        DELETE FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[&source_id, &target_id, &RelationshipType::HideReposts],
    ).await?;
    Ok(())
}

pub async fn hide_replies(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ON CONFLICT (source_id, target_id, relationship_type) DO NOTHING
        ",
        &[&source_id, &target_id, &RelationshipType::HideReplies],
    ).await?;
    Ok(())
}

pub async fn show_replies(
    db_client: &impl DatabaseClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    // Does not return NotFound error
    db_client.execute(
        "
        DELETE FROM relationship
        WHERE
            source_id = $1 AND target_id = $2
            AND relationship_type = $3
        ",
        &[&source_id, &target_id, &RelationshipType::HideReplies],
    ).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::{
        test_utils::create_test_database,
        DatabaseError,
    };
    use crate::profiles::{
        queries::create_profile,
        types::{DbActor, ProfileCreateData},
    };
    use crate::users::{
        queries::create_user,
        types::UserCreateData,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_follow_remote_profile() {
        let db_client = &mut create_test_database().await;
        let source_data = UserCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let source = create_user(db_client, source_data).await.unwrap();
        let target_actor_id = "https://example.org/users/1";
        let target_data = ProfileCreateData {
            username: "followed".to_string(),
            hostname: Some("example.org".to_string()),
            actor_json: Some(DbActor {
                id: target_actor_id.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let target = create_profile(db_client, target_data).await.unwrap();
        // Create follow request
        let follow_request = create_follow_request(db_client, &source.id, &target.id)
            .await.unwrap();
        assert_eq!(follow_request.source_id, source.id);
        assert_eq!(follow_request.target_id, target.id);
        assert_eq!(follow_request.activity_id, None);
        assert_eq!(follow_request.request_status, FollowRequestStatus::Pending);
        let following = get_following(db_client, &source.id).await.unwrap();
        assert!(following.is_empty());
        // Accept follow request
        follow_request_accepted(db_client, &follow_request.id).await.unwrap();
        let follow_request = get_follow_request_by_id(db_client, &follow_request.id)
            .await.unwrap();
        assert_eq!(follow_request.request_status, FollowRequestStatus::Accepted);
        let following = get_following(db_client, &source.id).await.unwrap();
        assert_eq!(following[0].id, target.id);
        let target_has_followers =
            has_local_followers(db_client, target_actor_id).await.unwrap();
        assert_eq!(target_has_followers, true);

        // Unfollow
        let follow_request_id = unfollow(db_client, &source.id, &target.id)
            .await.unwrap().unwrap();
        assert_eq!(follow_request_id, follow_request.id);
        let follow_request_result =
            get_follow_request_by_id(db_client, &follow_request_id).await;
        assert!(matches!(
            follow_request_result,
            Err(DatabaseError::NotFound("follow request")),
        ));
        let following = get_following(db_client, &source.id).await.unwrap();
        assert!(following.is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn test_followed_by_remote_profile() {
        let db_client = &mut create_test_database().await;
        let source_data = ProfileCreateData {
            username: "follower".to_string(),
            hostname: Some("example.org".to_string()),
            actor_json: Some(DbActor::default()),
            ..Default::default()
        };
        let source = create_profile(db_client, source_data).await.unwrap();
        let target_data = UserCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let target = create_user(db_client, target_data).await.unwrap();
        // Create follow request
        let activity_id = "https://example.org/objects/123";
        let _follow_request = create_remote_follow_request_opt(
            db_client, &source.id, &target.id, activity_id,
        ).await.unwrap();
        // Repeat
        let follow_request = create_remote_follow_request_opt(
            db_client, &source.id, &target.id, activity_id,
        ).await.unwrap();
        assert_eq!(follow_request.source_id, source.id);
        assert_eq!(follow_request.target_id, target.id);
        assert_eq!(follow_request.activity_id, Some(activity_id.to_string()));
        assert_eq!(follow_request.request_status, FollowRequestStatus::Pending);
        // Accept follow request
        follow_request_accepted(db_client, &follow_request.id).await.unwrap();
        let follow_request = get_follow_request_by_id(db_client, &follow_request.id)
            .await.unwrap();
        assert_eq!(follow_request.request_status, FollowRequestStatus::Accepted);
    }
}
