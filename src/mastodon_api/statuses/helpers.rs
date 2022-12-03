use tokio_postgres::GenericClient;

use crate::database::DatabaseError;
use crate::models::posts::helpers::{
    add_user_actions,
    add_related_posts,
};
use crate::models::posts::types::Post;
use crate::models::users::types::User;
use super::types::Status;

/// Load related objects and build status for API response
pub async fn build_status(
    db_client: &impl GenericClient,
    instance_url: &str,
    user: Option<&User>,
    mut post: Post,
) -> Result<Status, DatabaseError> {
    add_related_posts(db_client, vec![&mut post]).await?;
    if let Some(user) = user {
        add_user_actions(db_client, &user.id, vec![&mut post]).await?;
    };
    let status = Status::from_post(post, instance_url);
    Ok(status)
}

pub async fn build_status_list(
    db_client: &impl GenericClient,
    instance_url: &str,
    user: Option<&User>,
    mut posts: Vec<Post>,
) -> Result<Vec<Status>, DatabaseError> {
    add_related_posts(db_client, posts.iter_mut().collect()).await?;
    if let Some(user) = user {
        add_user_actions(db_client, &user.id, posts.iter_mut().collect()).await?;
    };
    let statuses: Vec<Status> = posts
        .into_iter()
        .map(|post| Status::from_post(post, instance_url))
        .collect();
    Ok(statuses)
}
