use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    fetcher::helpers::ImportError,
    identifiers::parse_local_actor_id,
    receiver::find_object_id,
    vocabulary::PERSON,
};
use crate::config::Config;
use crate::errors::{DatabaseError, ValidationError};
use crate::models::notifications::queries::{
    create_subscription_expiration_notification,
};
use crate::models::profiles::queries::get_profile_by_actor_id;
use crate::models::relationships::queries::unsubscribe;
use crate::models::users::queries::get_user_by_name;
use super::HandlerResult;

pub async fn handle_remove(
    config: &Config,
    db_client: &impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    let actor_profile = get_profile_by_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let actor = actor_profile.actor_json.ok_or(ImportError::LocalObject)?;
    let target_value = activity.target.ok_or(ValidationError("target is missing"))?;
    let target_id = find_object_id(&target_value)?;
    if Some(target_id) == actor.subscribers {
        // Removing from subscribers
        let object_id = find_object_id(&activity.object)?;
        let username = parse_local_actor_id(&config.instance_url(), &object_id)?;
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
