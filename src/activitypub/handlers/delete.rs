use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    receiver::get_object_id,
    vocabulary::{NOTE, PERSON},
};
use crate::config::Config;
use crate::errors::{DatabaseError, ValidationError};
use crate::models::posts::queries::{delete_post, get_post_by_object_id};
use crate::models::profiles::queries::{
    delete_profile,
    get_profile_by_actor_id,
};
use super::HandlerResult;

pub async fn handle_delete(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    let object_id = get_object_id(&activity.object)?;
    if object_id == activity.actor {
        // Self-delete
        let profile = match get_profile_by_actor_id(db_client, &object_id).await {
            Ok(profile) => profile,
            // Ignore Delete(Person) if profile is not found
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
        let deletion_queue = delete_profile(db_client, &profile.id).await?;
        let config = config.clone();
        actix_rt::spawn(async move {
            deletion_queue.process(&config).await;
        });
        log::info!("deleted profie {}", profile.acct);
        return Ok(Some(PERSON));
    };
    let post = match get_post_by_object_id(db_client, &object_id).await {
        Ok(post) => post,
        // Ignore Delete(Note) if post is not found
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let actor_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
    if post.author.id != actor_profile.id {
        return Err(ValidationError("actor is not an author").into());
    };
    let deletion_queue = delete_post(db_client, &post.id).await?;
    let config = config.clone();
    actix_rt::spawn(async move {
        deletion_queue.process(&config).await;
    });
    Ok(Some(NOTE))
}
