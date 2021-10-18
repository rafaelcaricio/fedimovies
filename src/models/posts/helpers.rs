use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::errors::DatabaseError;
use crate::models::reactions::queries::get_favourited;
use super::types::{Post, PostActions};

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
