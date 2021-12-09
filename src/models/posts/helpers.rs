use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::reactions::queries::find_favourited_by_user;
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

pub fn can_view_post(user: Option<&User>, post: &Post) -> bool {
    match post.visibility {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_view_post_anonymous() {
        let post = Post {
            visibility: Visibility::Public,
            ..Default::default()
        };
        assert!(can_view_post(None, &post));
    }

    #[test]
    fn test_can_view_post_direct() {
        let user = User::default();
        let post = Post {
            visibility: Visibility::Direct,
            ..Default::default()
        };
        assert!(!can_view_post(Some(&user), &post));
    }

    #[test]
    fn test_can_view_post_direct_mentioned() {
        let user = User::default();
        let post = Post {
            visibility: Visibility::Direct,
            mentions: vec![user.profile.clone()],
            ..Default::default()
        };
        assert!(can_view_post(Some(&user), &post));
    }
}
