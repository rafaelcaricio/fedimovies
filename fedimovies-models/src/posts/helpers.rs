use uuid::Uuid;

use crate::database::{DatabaseClient, DatabaseError};
use crate::reactions::queries::find_favourited_by_user;
use crate::relationships::{queries::has_relationship, types::RelationshipType};
use crate::users::types::{Permission, User};

use super::queries::{find_reposted_by_user, get_post_by_id, get_related_posts};
use super::types::{Post, PostActions, Visibility};

pub async fn add_related_posts(
    db_client: &impl DatabaseClient,
    posts: Vec<&mut Post>,
) -> Result<(), DatabaseError> {
    let posts_ids = posts.iter().map(|post| post.id).collect();
    let related = get_related_posts(db_client, posts_ids).await?;
    let get_post = |post_id: &Uuid| -> Result<Post, DatabaseError> {
        let post = related
            .iter()
            .find(|post| post.id == *post_id)
            .ok_or(DatabaseError::NotFound("post"))?
            .clone();
        Ok(post)
    };
    for post in posts {
        if let Some(ref in_reply_to_id) = post.in_reply_to_id {
            let in_reply_to = get_post(in_reply_to_id)?;
            post.in_reply_to = Some(Box::new(in_reply_to));
        };
        if let Some(ref repost_of_id) = post.repost_of_id {
            let mut repost_of = get_post(repost_of_id)?;
            for linked_id in repost_of.links.iter() {
                let linked = get_post(linked_id)?;
                repost_of.linked.push(linked);
            }
            post.repost_of = Some(Box::new(repost_of));
        };
        for linked_id in post.links.iter() {
            let linked = get_post(linked_id)?;
            post.linked.push(linked);
        }
    }
    Ok(())
}

pub async fn add_user_actions(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
    posts: Vec<&mut Post>,
) -> Result<(), DatabaseError> {
    let posts_ids: Vec<Uuid> = posts
        .iter()
        .map(|post| post.id)
        .chain(
            posts
                .iter()
                .filter_map(|post| post.repost_of.as_ref())
                .map(|post| post.id),
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
    db_client: &impl DatabaseClient,
    user: Option<&User>,
    post: &Post,
) -> Result<bool, DatabaseError> {
    let is_mentioned = |user: &User| {
        post.mentions
            .iter()
            .any(|profile| profile.id == user.profile.id)
    };
    let result = match post.visibility {
        Visibility::Public => true,
        Visibility::Direct => {
            if let Some(user) = user {
                // Returns true if user is mentioned
                is_mentioned(user)
            } else {
                false
            }
        }
        Visibility::Followers => {
            if let Some(user) = user {
                let is_following = has_relationship(
                    db_client,
                    &user.id,
                    &post.author.id,
                    RelationshipType::Follow,
                )
                .await?;
                is_following || is_mentioned(user)
            } else {
                false
            }
        }
        Visibility::Subscribers => {
            if let Some(user) = user {
                // Can view only if mentioned
                is_mentioned(user)
            } else {
                false
            }
        }
    };
    Ok(result)
}

pub fn can_create_post(user: &User) -> bool {
    user.role.has_permission(Permission::CreatePost)
}

pub async fn get_local_post_by_id(
    db_client: &impl DatabaseClient,
    post_id: &Uuid,
) -> Result<Post, DatabaseError> {
    let post = get_post_by_id(db_client, post_id).await?;
    if !post.is_local() {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(post)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::test_utils::create_test_database;
    use crate::posts::{queries::create_post, types::PostCreateData};
    use crate::relationships::queries::{follow, subscribe};
    use crate::users::{
        queries::create_user,
        types::{Role, User, UserCreateData},
    };
    use serial_test::serial;
    use tokio_postgres::Client;

    async fn create_test_user(db_client: &mut Client, username: &str) -> User {
        let user_data = UserCreateData {
            username: username.to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        create_user(db_client, user_data).await.unwrap()
    }

    #[tokio::test]
    #[serial]
    async fn test_add_related_posts() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post_data = PostCreateData {
            content: "post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, &author.id, post_data).await.unwrap();
        let reply_data = PostCreateData {
            content: "reply".to_string(),
            in_reply_to_id: Some(post.id),
            ..Default::default()
        };
        let mut reply = create_post(db_client, &author.id, reply_data)
            .await
            .unwrap();
        add_related_posts(db_client, vec![&mut reply])
            .await
            .unwrap();
        assert_eq!(reply.in_reply_to.unwrap().id, post.id);
        assert!(reply.repost_of.is_none());
        assert!(reply.linked.is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_anonymous() {
        let post = Post {
            visibility: Visibility::Public,
            ..Default::default()
        };
        let db_client = &create_test_database().await;
        let result = can_view_post(db_client, None, &post).await.unwrap();
        assert!(result);
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
        assert!(!result);
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
        assert!(result);
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
        assert!(!result);
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
        let result = can_view_post(db_client, Some(&follower), &post)
            .await
            .unwrap();
        assert!(result);
    }

    #[tokio::test]
    #[serial]
    async fn test_can_view_post_subscribers_only() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let follower = create_test_user(db_client, "follower").await;
        follow(db_client, &follower.id, &author.id).await.unwrap();
        let subscriber = create_test_user(db_client, "subscriber").await;
        subscribe(db_client, &subscriber.id, &author.id)
            .await
            .unwrap();
        let post = Post {
            author: author.profile,
            visibility: Visibility::Subscribers,
            mentions: vec![subscriber.profile.clone()],
            ..Default::default()
        };
        assert!(!can_view_post(db_client, None, &post).await.unwrap(),);
        assert!(!can_view_post(db_client, Some(&follower), &post)
            .await
            .unwrap(),);
        assert!(can_view_post(db_client, Some(&subscriber), &post)
            .await
            .unwrap(),);
    }

    #[test]
    fn test_can_create_post() {
        let mut user = User {
            role: Role::NormalUser,
            ..Default::default()
        };
        assert!(can_create_post(&user));
        user.role = Role::ReadOnlyUser;
        assert!(!can_create_post(&user));
    }
}
