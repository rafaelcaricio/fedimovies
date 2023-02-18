use serde::Deserialize;
use serde_json::Value;

use mitra_config::Config;

use crate::activitypub::{
    identifiers::parse_local_actor_id,
    vocabulary::PERSON,
};
use crate::database::{DatabaseClient, DatabaseError};
use crate::errors::ValidationError;
use crate::models::{
    notifications::queries::{
        create_subscription_expiration_notification,
    },
    profiles::queries::get_profile_by_remote_actor_id,
    relationships::queries::unsubscribe,
    users::queries::get_user_by_name,
};
use super::{HandlerError, HandlerResult};

#[derive(Deserialize)]
struct Remove {
    actor: String,
    object: String,
    target: String,
}

pub async fn handle_remove(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    let activity: Remove = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let actor = actor_profile.actor_json.ok_or(HandlerError::LocalObject)?;
    if Some(activity.target) == actor.subscribers {
        // Removing from subscribers
        let username = parse_local_actor_id(
            &config.instance_url(),
            &activity.object,
        )?;
        let user = get_user_by_name(db_client, &username).await?;
        // actor is recipient, user is sender
        match unsubscribe(db_client, &user.id, &actor_profile.id).await {
            Ok(_) => {
                create_subscription_expiration_notification(
                    db_client,
                    &actor_profile.id,
                    &user.id,
                ).await?;
                return Ok(Some(PERSON));
            },
            // Ignore removal if relationship does not exist
            Err(DatabaseError::NotFound(_)) => return Ok(None),
            Err(other_error) => return Err(other_error.into()),
        };
    };
    Ok(None)
}
