use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::reactions::queries::find_favourited_by_user;
use crate::models::relationships::queries::get_relationship;
use crate::models::users::types::User;
use super::queries::{get_posts, find_reposted_by_user};
use super::types::{Post, PostActions, Visibility};

pub async fn get_reposted_posts(
    db_client: &impl GenericClient,
    posts: Vec<&mut Post>,
) -> Result<(), DatabaseError> {
    let reposted_ids: Vec<Uuid> = posts.iter()
        .filter_map(|post| post.repost_of_id)
        .collect();
    let reposted = get_posts(db_client, reposted_ids).await?;
    for post in posts {
        if let Some(ref repost_of_id) = post.repost_of_id {
            let repost_of = reposted.iter()
                .find(|post| post.id == *repost_of_id)
                .ok_or(DatabaseError::NotFound("post"))?
                .clone();
            post.repost_of = Some(Box::new(repost_of));
        };
    };
    Ok(())
}

pub async fn get_actions_for_posts(
    db_client: &impl GenericClient,
    user_id: &Uuid,
    posts: Vec<&mut Post>,
) -> Result<(), DatabaseError> {
    let posts_ids: Vec<Uuid> = posts.iter()
        .map(|post| post.id)
        .chain(
            posts.iter()
                .filter_map(|post| post.repost_of.as_ref())
                .map(|post| post.id)
        )
        .collect();
    let favourites = find_favourited_by_user(db_client, user_id, &posts_ids).await?;
    let reposted = find_reposted_by_user(db_client, user_id, &posts_ids).await?;
    for post in posts {
        if let Some(ref mut repost_of) = post.repost_of {
            let actions = PostActions {
                favourited: favourites.contains(&repost_of.id),
                reposted: reposted.contains(&repost_of.id),
            };
            repost_of.actions = Some(actions);
        };
        let actions = PostActions {
            favourited: favourites.contains(&post.id),
            reposted: reposted.contains(&post.id),
        };
        post.actions = Some(actions);
    }
    Ok(())
}

pub async fn can_view_post(
    db_client: &impl GenericClient,
    user: Option<&User>,
    post: &Post,
) -> Result<bool, DatabaseError> {
    let result = match post.visibility {
        Visibility::Public => true,
        Visibility::Direct => {
            if let Some(user) = user {
                // Returns true if user is mentioned
                post.mentions.iter()
                    .any(|profile| profile.id == user.profile.id)
            } else {
                false
            }
        },
        Visibility::Followers => {
            if let Some(user) = user {
                let relationship = get_relationship(
                    db_client,
                    &post.author.id,
                    &user.id,
                ).await?;
                let is_mentioned = post.mentions.iter()
                    .any(|profile| profile.id == user.profile.id);
                relationship.followed_by || is_mentioned
            } else {
                false
            }
        },
        Visibility::Subscribers => {
            if let Some(user) = user {
                let relationship = get_relationship(
                    db_client,
                    &post.author.id,
                    &user.id,
                ).await?;
                let is_mentioned = post.mentions.iter()
                    .any(|profile| profile.id == user.profile.id);
                relationship.subscription_from || is_mentioned
            } else {
                false
            }
        },
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use tokio_postgres::Client;
    use crate::database::test_utils::create_test_database;
    use crate::models::relationships::queries::{follow, subscribe};
    use crate::models::users::queries::create_user;
    use crate::models::users::types::UserCreateData;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_anonymous() {
        let post = Post {
            visibility: Visibility::Public,
            ..Default::default()
        };
        let db_client = &create_test_database().await;
        let result = can_view_post(db_client, None, &post).await.unwrap();
        assert_eq!(result, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_direct() {
        let user = User::default();
        let post = Post {
            visibility: Visibility::Direct,
            ..Default::default()
        };
        let db_client = &create_test_database().await;
        let result = can_view_post(db_client, Some(&user), &post).await.unwrap();
        assert_eq!(result, false);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_direct_mentioned() {
        let user = User::default();
        let post = Post {
            visibility: Visibility::Direct,
            mentions: vec![user.profile.clone()],
            ..Default::default()
        };
        let db_client = &create_test_database().await;
        let result = can_view_post(db_client, Some(&user), &post).await.unwrap();
        assert_eq!(result, true);
    }

    async fn create_test_user(db_client: &mut Client, username: &str) -> User {
        let user_data = UserCreateData {
            username: username.to_string(),
            ..Default::default()
        };
        create_user(db_client, user_data).await.unwrap()
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_followers_only_anonymous() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let post = Post {
            author: author.profile,
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let result = can_view_post(db_client, None, &post).await.unwrap();
        assert_eq!(result, false);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_followers_only_follower() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let follower = create_test_user(db_client, "follower").await;
        follow(db_client, &follower.id, &author.id).await.unwrap();
        let post = Post {
            author: author.profile,
            visibility: Visibility::Followers,
            ..Default::default()
        };
        let result = can_view_post(db_client, Some(&follower), &post).await.unwrap();
        assert_eq!(result, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_subscribers_only() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let follower = create_test_user(db_client, "follower").await;
        follow(db_client, &follower.id, &author.id).await.unwrap();
        let subscriber = create_test_user(db_client, "subscriber").await;
        subscribe(db_client, &subscriber.id, &author.id).await.unwrap();
        let post = Post {
            author: author.profile,
            visibility: Visibility::Subscribers,
            ..Default::default()
        };
        assert_eq!(
            can_view_post(db_client, None, &post).await.unwrap(),
            false,
        );
        assert_eq!(
            can_view_post(db_client, Some(&follower), &post).await.unwrap(),
            false,
        );
        assert_eq!(
            can_view_post(db_client, Some(&subscriber), &post).await.unwrap(),
            true,
        );
    }
}
