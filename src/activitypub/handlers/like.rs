use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    fetcher::helpers::get_or_import_profile_by_actor_id,
    receiver::{get_object_id, parse_object_id},
    vocabulary::NOTE,
};
use crate::config::Config;
use crate::errors::DatabaseError;
use crate::models::reactions::queries::create_reaction;
use crate::models::posts::queries::get_post_by_object_id;
use super::HandlerResult;

pub async fn handle_like(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    let author = get_or_import_profile_by_actor_id(
        db_client,
        &config.instance(),
        &config.media_dir(),
        &activity.actor,
    ).await?;
    let object_id = get_object_id(&activity.object)?;
    let post_id = match parse_object_id(&config.instance_url(), &object_id) {
        Ok(post_id) => post_id,
        Err(_) => {
            let post = match get_post_by_object_id(db_client, &object_id).await {
                Ok(post) => post,
                // Ignore like if post is not found locally
                Err(DatabaseError::NotFound(_)) => return Ok(None),
                Err(other_error) => return Err(other_error.into()),
            };
            post.id
        },
    };
    match create_reaction(
        db_client,
        &author.id,
        &post_id,
        Some(&activity.id),
    ).await {
        Ok(_) => (),
        // Ignore activity if reaction is already saved
        Err(DatabaseError::AlreadyExists(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(Some(NOTE))
}
