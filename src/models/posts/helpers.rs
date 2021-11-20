use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::reactions::queries::get_favourited;
use crate::models::users::types::User;
use super::types::{Post, PostActions, Visibility};

pub async fn get_actions_for_post(
    db_client: &impl GenericClient,
    user_id: &Uuid,
    post: &mut Post,
) -> Result<(), DatabaseError> {
    let favourited = get_favourited(db_client, user_id, vec![post.id]).await?;
    let actions = PostActions { favourited: favourited.contains(&post.id) };
    post.actions = Some(actions);
    Ok(())
}

pub async fn get_actions_for_posts(
    db_client: &impl GenericClient,
    user_id: &Uuid,
    posts: Vec<&mut Post>,
) -> Result<(), DatabaseError> {
    let posts_ids = posts.iter().map(|post| post.id).collect();
    let favourited = get_favourited(db_client, user_id, posts_ids).await?;
    for post in posts {
        let actions = PostActions { favourited: favourited.contains(&post.id) };
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
