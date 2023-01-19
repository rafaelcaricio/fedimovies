use serde::Deserialize;
use serde_json::Value;

use crate::activitypub::{
    receiver::deserialize_into_object_id,
    vocabulary::{NOTE, PERSON},
};
use crate::config::Config;
use crate::database::{DatabaseClient, DatabaseError};
use crate::errors::ValidationError;
use crate::models::{
    posts::queries::{
        delete_post,
        get_post_by_remote_object_id,
    },
    profiles::queries::{
        delete_profile,
        get_profile_by_remote_actor_id,
    },
};
use super::HandlerResult;

#[derive(Deserialize)]
struct Delete {
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_delete(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    let activity: Delete = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    if activity.object == activity.actor {
        // Self-delete
        let profile = match get_profile_by_remote_actor_id(
            db_client,
            &activity.object,
        ).await {
            Ok(profile) => profile,
            // Ignore Delete(Person) if profile is not found
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        let deletion_queue = delete_profile(db_client, &profile.id).await?;
        let config = config.clone();
        tokio::spawn(async move {
            deletion_queue.process(&config).await;
        });
        log::info!("deleted profile {}", profile.acct);
        return Ok(Some(PERSON));
    };
    let post = match get_post_by_remote_object_id(
        db_client,
        &activity.object,
    ).await {
        Ok(post) => post,
        // Ignore Delete(Note) if post is not found
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    if post.author.id != actor_profile.id {
        return Err(ValidationError("actor is not an author").into());
    };
    let deletion_queue = delete_post(db_client, &post.id).await?;
    let config = config.clone();
    tokio::spawn(async move {
        deletion_queue.process(&config).await;
    });
    Ok(Some(NOTE))
}
