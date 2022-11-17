use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    builders::{
        follow::prepare_follow,
        undo_follow::prepare_undo_follow,
    },
    fetcher::helpers::get_or_import_profile_by_actor_id,
    receiver::{find_object_id, parse_array},
    vocabulary::PERSON,
};
use crate::config::Config;
use crate::errors::{DatabaseError, ValidationError};
use crate::models::{
    notifications::queries::create_move_notification,
    relationships::queries::{
        create_follow_request,
        get_followers,
        unfollow,
    },
    users::queries::get_user_by_id,
};
use super::HandlerResult;

pub async fn handle_move_person(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    let object_id = find_object_id(&activity.object)?;
    if object_id != activity.actor {
        return Err(ValidationError("actor ID mismatch").into());
    };
    let target_value = activity.target
        .ok_or(ValidationError("target is missing"))?;
    let target_id = find_object_id(&target_value)?;

    let instance = config.instance();
    let media_dir = config.media_dir();
    let old_profile = get_or_import_profile_by_actor_id(
        db_client,
        &instance,
        &media_dir,
        &activity.actor,
    ).await?;
    let old_actor = old_profile.actor_json.unwrap();
    let new_profile = get_or_import_profile_by_actor_id(
        db_client,
        &instance,
        &media_dir,
        &target_id,
    ).await?;
    let new_actor = new_profile.actor_json.unwrap();
    let maybe_also_known_as = new_actor.also_known_as.as_ref()
        .and_then(|value| parse_array(value).ok())
        .and_then(|aliases| aliases.first().cloned());
    if maybe_also_known_as.as_ref() != Some(&old_actor.id) {
        return Err(ValidationError("target ID is not an alias").into());
    };

    let followers = get_followers(db_client, &old_profile.id).await?;
    let mut activities = vec![];
    for follower in followers {
        let follower = get_user_by_id(db_client, &follower.id).await?;
        // Unfollow old profile
        let maybe_follow_request_id = unfollow(
            db_client,
            &follower.id,
            &old_profile.id,
        ).await?;
        // The target is remote profile, so follow request must exist
        let follow_request_id = maybe_follow_request_id.unwrap();
        activities.push(prepare_undo_follow(
            &instance,
            &follower,
            &old_actor,
            &follow_request_id,
        ));
        // Follow new profile
        match create_follow_request(
            db_client,
            &follower.id,
            &new_profile.id,
        ).await {
            Ok(follow_request) => {
                activities.push(prepare_follow(
                    &instance,
                    &follower,
                    &new_actor,
                    &follow_request.id,
                ));
            },
            Err(DatabaseError::AlreadyExists(_)) => (), // already following
            Err(other_error) => return Err(other_error.into()),
        };
        create_move_notification(
            db_client,
            &new_profile.id,
            &follower.id,
        ).await?;
    };
    tokio::spawn(async move {
        for activity in activities {
            activity.deliver_or_log().await;
        };
    });

    Ok(Some(PERSON))
}
