use serde::Deserialize;
use serde_json::Value;

use crate::activitypub::{
    fetcher::helpers::get_or_import_profile_by_actor_id,
    receiver::deserialize_into_object_id,
    vocabulary::NOTE,
};
use crate::config::Config;
use crate::database::{DatabaseClient, DatabaseError};
use crate::errors::ValidationError;
use crate::models::{
    reactions::queries::create_reaction,
    posts::helpers::get_post_by_object_id,
};
use super::HandlerResult;

#[derive(Deserialize)]
struct Like {
    id: String,
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_like(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    let activity: Like = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let author = get_or_import_profile_by_actor_id(
        db_client,
        &config.instance(),
        &config.media_dir(),
        &activity.actor,
    ).await?;
    let post_id = match get_post_by_object_id(
        db_client,
        &config.instance_url(),
        &activity.object,
    ).await {
        Ok(post) => post.id,
        // Ignore like if post is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
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
