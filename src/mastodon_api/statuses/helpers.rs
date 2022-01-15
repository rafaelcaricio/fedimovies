use tokio_postgres::GenericClient;

use crate::activitypub::actor::Actor;
use crate::errors::DatabaseError;
use crate::models::posts::queries::get_post_author;
use crate::models::posts::types::{Post, Visibility};
use crate::models::relationships::queries::{get_followers, get_subscribers};
use crate::models::users::types::User;

pub async fn get_note_recipients(
    db_client: &impl GenericClient,
    current_user: &User,
    post: &Post,
) -> Result<Vec<Actor>, DatabaseError> {
    let mut audience = vec![];
    match post.visibility {
        Visibility::Public | Visibility::Followers => {
            let followers = get_followers(db_client, &current_user.id, None, None).await?;
            audience.extend(followers);
        },
        Visibility::Subscribers => {
            let subscribers = get_subscribers(db_client, &current_user.id).await?;
            audience.extend(subscribers);
        },
        Visibility::Direct => (),
    };
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
