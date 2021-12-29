use tokio_postgres::GenericClient;

use crate::activitypub::actor::Actor;
use crate::errors::DatabaseError;
use crate::models::posts::queries::get_post_author;
use crate::models::posts::types::Post;
use crate::models::relationships::queries::get_followers;
use crate::models::users::types::User;

pub async fn get_note_audience(
    db_client: &impl GenericClient,
    current_user: &User,
    post: &Post,
) -> Result<Vec<Actor>, DatabaseError> {
    let mut audience = get_followers(db_client, &current_user.id, None, None).await?;
    if let Some(in_reply_to_id) = post.in_reply_to_id {
        // TODO: use post.in_reply_to ?
        let in_reply_to_author = get_post_author(db_client, &in_reply_to_id).await?;
        audience.push(in_reply_to_author);
    };
    audience.extend(post.mentions.clone());
    let mut recipients: Vec<Actor> = Vec::new();
    for profile in audience {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    Ok(recipients)
}

pub struct Audience {
    pub recipients: Vec<Actor>,
    pub primary_recipient: Option<String>,
}

pub async fn get_like_audience(
    _db_client: &impl GenericClient,
    post: &Post,
) -> Result<Audience, DatabaseError> {
    let mut recipients: Vec<Actor> = Vec::new();
    let mut primary_recipient = None;
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        primary_recipient = Some(remote_actor.id.clone());
        recipients.push(remote_actor.clone());
    };
    Ok(Audience { recipients, primary_recipient })
}

pub async fn get_announce_audience(
    db_client: &impl GenericClient,
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
    let mut primary_recipient = None;
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        primary_recipient = Some(remote_actor.id.clone());
        recipients.push(remote_actor.clone());
    };
    Ok(Audience { recipients, primary_recipient })
}