use uuid::Uuid;

use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::helpers::{
        find_declared_aliases,
        find_verified_aliases,
    },
    profiles::types::DbActorProfile,
    relationships::queries::{
        get_relationships,
    },
    relationships::types::RelationshipType,
};

use super::types::{Account, Aliases, RelationshipMap};

pub async fn get_relationship(
    db_client: &impl DatabaseClient,
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
            RelationshipType::HideReposts => {
                if relationship.is_direct(source_id, target_id)? {
                    relationship_map.showing_reblogs = false;
                };
            },
            RelationshipType::HideReplies => {
                if relationship.is_direct(source_id, target_id)? {
                    relationship_map.showing_replies = false;
                };
            },
        };
    };
    Ok(relationship_map)
}

pub async fn get_aliases(
    db_client: &impl DatabaseClient,
    base_url: &str,
    instance_url: &str,
    profile: &DbActorProfile,
) -> Result<Aliases, DatabaseError> {
    let declared = find_declared_aliases(db_client, profile).await?
        .into_iter()
        .map(|profile| Account::from_profile(
            base_url,
            instance_url,
            profile,
        ))
        .collect();
    let verified = find_verified_aliases(db_client, profile).await?
        .into_iter()
        .map(|profile| Account::from_profile(
            base_url,
            instance_url,
            profile,
        ))
        .collect();
    let aliases = Aliases { declared, verified };
    Ok(aliases)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use mitra_models::{
        database::test_utils::create_test_database,
        relationships::queries::{
            create_follow_request,
            follow,
            follow_request_accepted,
            hide_reposts,
            show_reposts,
            subscribe,
            unfollow,
            unsubscribe,
        },
        users::queries::create_user,
        users::types::{User, UserCreateData},
    };
    use super::*;

    async fn create_users(db_client: &mut impl DatabaseClient)
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
        assert_eq!(relationship.showing_reblogs, true);
        assert_eq!(relationship.showing_replies, true);
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

    #[tokio::test]
    #[serial]
    async fn test_hide_reblogs() {
        let db_client = &mut create_test_database().await;
        let (user_1, user_2) = create_users(db_client).await.unwrap();
        follow(db_client, &user_1.id, &user_2.id).await.unwrap();
        let relationship = get_relationship(db_client, &user_1.id, &user_2.id).await.unwrap();
        assert_eq!(relationship.following, true);
        assert_eq!(relationship.showing_reblogs, true);

        hide_reposts(db_client, &user_1.id, &user_2.id).await.unwrap();
        let relationship = get_relationship(db_client, &user_1.id, &user_2.id).await.unwrap();
        assert_eq!(relationship.following, true);
        assert_eq!(relationship.showing_reblogs, false);

        show_reposts(db_client, &user_1.id, &user_2.id).await.unwrap();
        let relationship = get_relationship(db_client, &user_1.id, &user_2.id).await.unwrap();
        assert_eq!(relationship.following, true);
        assert_eq!(relationship.showing_reblogs, true);
    }
}
