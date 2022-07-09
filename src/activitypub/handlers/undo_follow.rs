use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::{Activity, Object},
    receiver::parse_actor_id,
    vocabulary::FOLLOW,
};
use crate::config::Config;
use crate::errors::{DatabaseError, ValidationError};
use crate::models::profiles::queries::{
    get_profile_by_acct,
    get_profile_by_actor_id,
};
use crate::models::relationships::queries::unfollow;
use super::HandlerResult;

pub async fn handle_undo_follow(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    let object: Object = serde_json::from_value(activity.object)
        .map_err(|_| ValidationError("invalid object"))?;
    let source_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
    let target_actor_id = object.object
        .ok_or(ValidationError("invalid object"))?;
    let target_username = parse_actor_id(&config.instance_url(), &target_actor_id)?;
    // acct equals username if profile is local
    let target_profile = get_profile_by_acct(db_client, &target_username).await?;
    match unfollow(db_client, &source_profile.id, &target_profile.id).await {
        Ok(_) => (),
        // Ignore Undo if relationship doesn't exist
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(Some(FOLLOW))
}
