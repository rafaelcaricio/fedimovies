use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::identifiers::parse_local_object_id;
use crate::database::DatabaseError;
use crate::models::reactions::queries::find_favourited_by_user;
use crate::models::relationships::queries::has_relationship;
use crate::models::relationships::types::RelationshipType;
use crate::models::users::types::User;
use super::queries::{
    get_post_by_id,
    get_post_by_remote_object_id,
    get_related_posts,
    find_reposted_by_user,
};
use super::types::{Post, PostActions, Visibility};

pub async fn add_related_posts(
    db_client: &impl GenericClient,
    posts: Vec<&mut Post>,
) -> Result<(), DatabaseError> {
    let posts_ids = posts.iter().map(|post| post.id).collect();
    let related = get_related_posts(db_client, posts_ids).await?;
    let get_post = |post_id: &Uuid| -> Result<Post, DatabaseError> {
        let post = related.iter()
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
            };
            post.repost_of = Some(Box::new(repost_of));
        };
        for linked_id in post.links.iter() {
            let linked = get_post(linked_id)?;
            post.linked.push(linked);
        };
    };
    Ok(())
}

pub async fn add_user_actions(
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
    let is_mentioned = |user: &User| {
        post.mentions.iter().any(|profile| profile.id == user.profile.id)
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
        },
        Visibility::Followers => {
            if let Some(user) = user {
                let is_following = has_relationship(
                    db_client,
                    &user.id,
                    &post.author.id,
                    RelationshipType::Follow,
                ).await?;
                is_following || is_mentioned(user)
            } else {
                false
            }
        },
        Visibility::Subscribers => {
            if let Some(user) = user {
                // Can view only if mentioned
                is_mentioned(user)
            } else {
                false
            }
        },
    };
    Ok(result)
}

pub async fn get_local_post_by_id(
    db_client: &impl GenericClient,
    post_id: &Uuid,
) -> Result<Post, DatabaseError> {
    let post = get_post_by_id(db_client, post_id).await?;
    if !post.is_local() {
        return Err(DatabaseError::NotFound("post"));
    };
    Ok(post)
}

pub async fn get_post_by_object_id(
    db_client: &impl GenericClient,
    instance_url: &str,
    object_id: &str,
) -> Result<Post, DatabaseError> {
    match parse_local_object_id(instance_url, object_id) {
        Ok(post_id) => {
            // Local post
            let post = get_local_post_by_id(db_client, &post_id).await?;
            Ok(post)
        },
        Err(_) => {
            // Remote post
            let post = get_post_by_remote_object_id(db_client, object_id).await?;
            Ok(post)
        },
    }
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use tokio_postgres::Client;
    use crate::database::test_utils::create_test_database;
    use crate::models::posts::queries::create_post;
    use crate::models::posts::types::PostCreateData;
    use crate::models::relationships::queries::{follow, subscribe};
    use crate::models::users::queries::create_user;
    use crate::models::users::types::UserCreateData;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_add_related_posts() {
        let db_client = &mut create_test_database().await;
        let author_data = UserCreateData {
            username: "test".to_string(),
            ..Default::default()
        };
        let author = create_user(db_client, author_data).await.unwrap();
        let post_data = PostCreateData {
            content: "post".to_string(),
            ..Default::default()
        };
        let post = create_post(db_client, &author.id, post_data).await.unwrap();
        let reply_data = PostCreateData {
            content: "reply".to_string(),
            in_reply_to_id: Some(post.id.clone()),
            ..Default::default()
        };
        let mut reply = create_post(db_client, &author.id, reply_data).await.unwrap();
        add_related_posts(db_client, vec![&mut reply]).await.unwrap();
        assert_eq!(reply.in_reply_to.unwrap().id, post.id);
        assert_eq!(reply.repost_of.is_none(), true);
        assert_eq!(reply.linked.is_empty(), true);
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
            mentions: vec![subscriber.profile.clone()],
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
