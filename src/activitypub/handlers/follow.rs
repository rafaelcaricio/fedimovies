use serde::Deserialize;
use serde_json::Value;

use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    relationships::queries::{
        create_remote_follow_request_opt,
        follow_request_accepted,
    },
    users::queries::get_user_by_name,
};

use crate::activitypub::{
    builders::accept_follow::prepare_accept_follow,
    fetcher::helpers::get_or_import_profile_by_actor_id,
    identifiers::parse_local_actor_id,
    receiver::deserialize_into_object_id,
    vocabulary::PERSON,
};
use crate::errors::ValidationError;
use crate::media::MediaStorage;
use super::{HandlerError, HandlerResult};

#[derive(Deserialize)]
struct Follow {
    id: String,
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_follow(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    // Follow(Person)
    let activity: Follow = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let source_profile = get_or_import_profile_by_actor_id(
        db_client,
        &config.instance(),
        &MediaStorage::from(config),
        &activity.actor,
    ).await?;
    let source_actor = source_profile.actor_json
        .ok_or(HandlerError::LocalObject)?;
    let target_username = parse_local_actor_id(
        &config.instance_url(),
        &activity.object,
    )?;
    let target_user = get_user_by_name(db_client, &target_username).await?;
    let follow_request = create_remote_follow_request_opt(
        db_client,
        &source_profile.id,
        &target_user.id,
        &activity.id,
    ).await?;
    match follow_request_accepted(db_client, &follow_request.id).await {
        Ok(_) => (),
        // Proceed even if relationship already exists
        Err(DatabaseError::AlreadyExists(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };

    // Send Accept(Follow)
    prepare_accept_follow(
        &config.instance(),
        &target_user,
        &source_actor,
        &activity.id,
    ).enqueue(db_client).await?;

    Ok(Some(PERSON))
}
