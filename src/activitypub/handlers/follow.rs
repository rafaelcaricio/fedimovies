use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::{create_activity_accept_follow, Activity},
    deliverer::deliver_activity,
    fetcher::helpers::{get_or_import_profile_by_actor_id, ImportError},
    receiver::{get_object_id, parse_actor_id},
    vocabulary::PERSON,
};
use crate::config::Config;
use crate::errors::DatabaseError;
use crate::models::relationships::queries::follow;
use crate::models::users::queries::get_user_by_name;
use super::HandlerResult;

pub async fn handle_follow(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    let source_profile = get_or_import_profile_by_actor_id(
        db_client,
        &config.instance(),
        &config.media_dir(),
        &activity.actor,
    ).await?;
    let source_actor = source_profile.actor_json
        .ok_or(ImportError::LocalObject)?;
    let target_actor_id = get_object_id(&activity.object)?;
    let target_username = parse_actor_id(&config.instance_url(), &target_actor_id)?;
    let target_user = get_user_by_name(db_client, &target_username).await?;
    match follow(db_client, &source_profile.id, &target_user.profile.id).await {
        Ok(_) => (),
        // Proceed even if relationship already exists
        Err(DatabaseError::AlreadyExists(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };

    // Send activity
    let new_activity = create_activity_accept_follow(
        &config.instance_url(),
        &target_user.profile,
        &activity.id,
        &source_actor.id,
    );
    let recipients = vec![source_actor];
    deliver_activity(config, &target_user, new_activity, recipients);
    Ok(Some(PERSON))
}
