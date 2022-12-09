use serde::Deserialize;
use serde_json::Value;
use tokio_postgres::GenericClient;

use crate::activitypub::{
    identifiers::parse_local_actor_id,
    vocabulary::PERSON,
};
use crate::config::Config;
use crate::errors::ValidationError;
use crate::models::profiles::queries::get_profile_by_remote_actor_id;
use crate::models::relationships::queries::subscribe_opt;
use crate::models::users::queries::get_user_by_name;
use super::{HandlerError, HandlerResult};

#[derive(Deserialize)]
struct Add {
    actor: String,
    object: String,
    target: String,
}

pub async fn handle_add(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Value,
) -> HandlerResult {
    let activity: Add = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let actor = actor_profile.actor_json.ok_or(HandlerError::LocalObject)?;
    if Some(activity.target) == actor.subscribers {
        // Adding to subscribers
        let username = parse_local_actor_id(
            &config.instance_url(),
            &activity.object,
        )?;
        let user = get_user_by_name(db_client, &username).await?;
        subscribe_opt(db_client, &user.id, &actor_profile.id).await?;
        return Ok(Some(PERSON));
    };
    Ok(None)
}
