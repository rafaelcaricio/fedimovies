use tokio_postgres::GenericClient;

use crate::activitypub::actor::Actor;
use crate::errors::DatabaseError;
use crate::models::posts::helpers::{
    add_user_actions,
    add_reposted_posts,
};
use crate::models::posts::types::Post;
use crate::models::relationships::queries::get_followers;
use crate::models::users::types::User;
use super::types::Status;

pub struct Audience {
    pub recipients: Vec<Actor>,
    pub primary_recipient: String,
}

pub async fn get_like_recipients(
    _db_client: &impl GenericClient,
    instance_url: &str,
    post: &Post,
) -> Result<Audience, DatabaseError> {
    let mut recipients: Vec<Actor> = Vec::new();
    let primary_recipient = post.author.actor_id(instance_url);
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        recipients.push(remote_actor.clone());
    };
    Ok(Audience { recipients, primary_recipient })
}

pub async fn get_announce_recipients(
    db_client: &impl GenericClient,
    instance_url: &str,
    current_user: &User,
    post: &Post,
) -> Result<Audience, DatabaseError> {
    let followers = get_followers(db_client, &current_user.id, None, None).await?;
    let mut recipients: Vec<Actor> = Vec::new();
    for profile in followers {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    let primary_recipient = post.author.actor_id(instance_url);
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        recipients.push(remote_actor.clone());
    };
    Ok(Audience { recipients, primary_recipient })
}

/// Load related objects and build status for API response
pub async fn build_status(
    db_client: &impl GenericClient,
    instance_url: &str,
    user: Option<&User>,
    mut post: Post,
) -> Result<Status, DatabaseError> {
    add_reposted_posts(db_client, vec![&mut post]).await?;
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
    add_reposted_posts(db_client, posts.iter_mut().collect()).await?;
    if let Some(user) = user {
        add_user_actions(db_client, &user.id, posts.iter_mut().collect()).await?;
    };
    let statuses: Vec<Status> = posts
        .into_iter()
        .map(|post| Status::from_post(post, instance_url))
        .collect();
    Ok(statuses)
}
