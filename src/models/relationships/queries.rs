use std::convert::TryFrom;

use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::models::notifications::queries::create_follow_notification;
use crate::models::profiles::queries::{
    update_follower_count,
    update_following_count,
};
use crate::models::profiles::types::DbActorProfile;
use crate::utils::id::new_uuid;
use super::types::{
    DbFollowRequest,
    DbRelationship,
    FollowRequestStatus,
    RelationshipMap,
    RelationshipType,
};

async fn get_relationships(
    db_client: &impl GenericClient,
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

pub async fn get_relationship(
    db_client: &impl GenericClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<RelationshipMap, DatabaseError> {
    // NOTE: this method returns relationship map even if target does not exist
    let relationships = get_relationships(db_client, source_id, target_id).await?;
    let mut relationship_map = RelationshipMap { id: *target_id, ..Default::default() };
    for relationship in relationships {
        match relationship.relationship_type {
            RelationshipType::Follow => {
                if relationship.is_direct(source_id, target_id)? {
                    relationship_map.following = true;
                } else {
                    relationship_map.followed_by = true;
                };
            },
            RelationshipType::FollowRequest => {
                if relationship.is_direct(source_id, target_id)? {
                    relationship_map.requested = true;
                };
            },
            RelationshipType::Subscription => {
                if relationship.is_direct(source_id, target_id)? {
                    relationship_map.subscription_to = true;
                } else {
                    relationship_map.subscription_from = true;
                };
            },
        };
    };
    Ok(relationship_map)
}

pub async fn follow(
    db_client: &mut impl GenericClient,
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
    db_client: &mut impl GenericClient,
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
        &[&source_id, &target_id, &RelationshipType::Follow],
    ).await?;
    let relationship_deleted = deleted_count > 0;
    // Delete follow request (for remote follows)
    let follow_request_deleted = delete_follow_request(
        &transaction,
        source_id,
        target_id,
    ).await?;
    if !relationship_deleted && !follow_request_deleted {
        return Err(DatabaseError::NotFound("relationship"));
    };
    if relationship_deleted {
        // Update counters only if relationship exists
        update_follower_count(&transaction, target_id, -1).await?;
        update_following_count(&transaction, source_id, -1).await?;
    }
    transaction.commit().await?;
    Ok(())
}

pub async fn create_follow_request(
    db_client: &impl GenericClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<DbFollowRequest, DatabaseError> {
    let request = DbFollowRequest {
        id: new_uuid(),
        source_id: source_id.to_owned(),
        target_id: target_id.to_owned(),
        request_status: FollowRequestStatus::Pending,
    };
    db_client.execute(
        "
        INSERT INTO follow_request (
            id, source_id, target_id, request_status
        )
        VALUES ($1, $2, $3, $4)
        ",
        &[
            &request.id,
            &request.source_id,
            &request.target_id,
            &request.request_status,
        ],
    ).await.map_err(catch_unique_violation("follow request"))?;
    Ok(request)
}

pub async fn follow_request_accepted(
    db_client: &mut impl GenericClient,
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
    db_client: &impl GenericClient,
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

pub async fn delete_follow_request(
    db_client: &impl GenericClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<bool, DatabaseError> {
    let deleted_count = db_client.execute(
        "
        DELETE FROM follow_request
        WHERE source_id = $1 AND target_id = $2
        ",
        &[&source_id, &target_id],
    ).await?;
    let is_success = deleted_count > 0;
    Ok(is_success)
}

pub async fn get_follow_request_by_id(
    db_client:  &impl GenericClient,
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

pub async fn get_follow_request_by_path(
    db_client: &impl GenericClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<DbFollowRequest, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT follow_request
        FROM follow_request
        WHERE source_id = $1 AND target_id = $2
        ",
        &[&source_id, &target_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("follow request"))?;
    let request: DbFollowRequest = row.try_get("follow_request")?;
    Ok(request)
}

pub async fn get_followers(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
    max_relationship_id: Option<i32>,
    limit: Option<i64>,
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
            AND ($3::integer IS NULL OR relationship.id < $3)
        ORDER BY relationship.id DESC
        LIMIT $4
        ",
        &[&profile_id, &RelationshipType::Follow, &max_relationship_id, &limit],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn get_following(
    db_client: &impl GenericClient,
    profile_id: &Uuid,
    max_relationship_id: Option<i32>,
    limit: Option<i64>,
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
            AND ($3::integer IS NULL OR relationship.id < $3)
        ORDER BY relationship.id DESC
        LIMIT $4
        ",
        &[&profile_id, &RelationshipType::Follow, &max_relationship_id, &limit],
    ).await?;
    let profiles = rows.iter()
        .map(|row| row.try_get("actor_profile"))
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn subscribe(
    db_client: &impl GenericClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ",
        &[&source_id, &target_id, &RelationshipType::Subscription],
    ).await.map_err(catch_unique_violation("relationship"))?;
    Ok(())
}

pub async fn subscribe_opt(
    db_client: &impl GenericClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        INSERT INTO relationship (source_id, target_id, relationship_type)
        VALUES ($1, $2, $3)
        ON CONFLICT (source_id, target_id, relationship_type) DO NOTHING
        ",
        &[&source_id, &target_id, &RelationshipType::Subscription],
    ).await?;
    Ok(())
}

pub async fn unsubscribe(
    db_client: &impl GenericClient,
    source_id: &Uuid,
    target_id: &Uuid,
) -> Result<(), DatabaseError> {
    let deleted_count = db_client.execute(
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::models::relationships::queries::follow;
    use crate::models::users::queries::create_user;
    use crate::models::users::types::{User, UserCreateData};
    use super::*;

    async fn create_users(db_client: &mut impl GenericClient)
        -> Result<(User, User), DatabaseError>
    {
        let user_data_1 = UserCreateData {
            username: "user".to_string(),
            ..Default::default()
        };
        let user_1 = create_user(db_client, user_data_1).await.unwrap();
        let user_data_2 = UserCreateData {
            username: "another-user".to_string(),
            ..Default::default()
        };
        let user_2 = create_user(db_client, user_data_2).await.unwrap();
        Ok((user_1, user_2))
    }

    #[tokio::test]
    #[serial]
    async fn test_follow_unfollow() {
        let db_client = &mut create_test_database().await;
        let (user_1, user_2) = create_users(db_client).await.unwrap();
        // Initial state
        let relationship = get_relationship(db_client, &user_1.id, &user_2.id).await.unwrap();
        assert_eq!(relationship.id, user_2.id);
        assert_eq!(relationship.following, false);
        assert_eq!(relationship.followed_by, false);
        assert_eq!(relationship.requested, false);
        assert_eq!(relationship.subscription_to, false);
        assert_eq!(relationship.subscription_from, false);
        // Follow request
        let follow_request = create_follow_request(db_client, &user_1.id, &user_2.id).await.unwrap();
        let relationship = get_relationship(db_client, &user_1.id, &user_2.id).await.unwrap();
        assert_eq!(relationship.following, false);
        assert_eq!(relationship.followed_by, false);
        assert_eq!(relationship.requested, true);
        // Mutual follow
        follow_request_accepted(db_client, &follow_request.id).await.unwrap();
        follow(db_client, &user_2.id, &user_1.id).await.unwrap();
        let relationship = get_relationship(db_client, &user_1.id, &user_2.id).await.unwrap();
        assert_eq!(relationship.following, true);
        assert_eq!(relationship.followed_by, true);
        assert_eq!(relationship.requested, false);
        // Unfollow
        unfollow(db_client, &user_1.id, &user_2.id).await.unwrap();
        let relationship = get_relationship(db_client, &user_1.id, &user_2.id).await.unwrap();
        assert_eq!(relationship.following, false);
        assert_eq!(relationship.followed_by, true);
        assert_eq!(relationship.requested, false);
    }

    #[tokio::test]
    #[serial]
    async fn test_subscribe_unsubscribe() {
        let db_client = &mut create_test_database().await;
        let (user_1, user_2) = create_users(db_client).await.unwrap();

        subscribe(db_client, &user_1.id, &user_2.id).await.unwrap();
        let relationship = get_relationship(db_client, &user_1.id, &user_2.id).await.unwrap();
        assert_eq!(relationship.subscription_to, true);
        assert_eq!(relationship.subscription_from, false);

        unsubscribe(db_client, &user_1.id, &user_2.id).await.unwrap();
        let relationship = get_relationship(db_client, &user_1.id, &user_2.id).await.unwrap();
        assert_eq!(relationship.subscription_to, false);
        assert_eq!(relationship.subscription_from, false);
    }
}
