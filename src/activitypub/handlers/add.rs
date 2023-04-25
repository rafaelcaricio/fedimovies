use serde::Deserialize;
use serde_json::Value;

use fedimovies_config::Config;
use fedimovies_models::{
    database::DatabaseClient, profiles::queries::get_profile_by_remote_actor_id,
    relationships::queries::subscribe_opt, users::queries::get_user_by_name,
};

use super::{HandlerError, HandlerResult};
use crate::activitypub::{identifiers::parse_local_actor_id, vocabulary::PERSON};
use crate::errors::ValidationError;

#[derive(Deserialize)]
struct Add {
    actor: String,
    object: String,
    target: String,
}

pub async fn handle_add(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    let activity: Add = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(db_client, &activity.actor).await?;
    let actor = actor_profile.actor_json.ok_or(HandlerError::LocalObject)?;
    if Some(activity.target) == actor.subscribers {
        // Adding to subscribers
        let username = parse_local_actor_id(&config.instance_url(), &activity.object)?;
        let user = get_user_by_name(db_client, &username).await?;
        subscribe_opt(db_client, &user.id, &actor_profile.id).await?;
        return Ok(Some(PERSON));
    };
    Ok(None)
}
