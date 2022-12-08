use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    identifiers::parse_local_actor_id,
    receiver::find_object_id,
    vocabulary::FOLLOW,
};
use crate::config::Config;
use crate::database::DatabaseError;
use crate::models::profiles::queries::{
    get_profile_by_acct,
    get_profile_by_remote_actor_id,
};
use crate::models::relationships::queries::unfollow;
use super::HandlerResult;

pub async fn handle_undo_follow(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    let source_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let target_actor_id = find_object_id(&activity.object["object"])?;
    let target_username = parse_local_actor_id(
        &config.instance_url(),
        &target_actor_id,
    )?;
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
