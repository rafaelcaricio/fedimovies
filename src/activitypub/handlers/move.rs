use serde::Deserialize;
use serde_json::Value;
use tokio_postgres::GenericClient;

use crate::activitypub::{
    builders::{
        follow::prepare_follow,
        undo_follow::prepare_undo_follow,
    },
    fetcher::helpers::get_or_import_profile_by_actor_id,
    identifiers::parse_local_actor_id,
    receiver::parse_array,
    vocabulary::PERSON,
};
use crate::config::Config;
use crate::database::DatabaseError;
use crate::errors::ValidationError;
use crate::models::{
    notifications::queries::create_move_notification,
    profiles::helpers::find_aliases,
    relationships::queries::{
        create_follow_request,
        get_followers,
        unfollow,
    },
    users::queries::{get_user_by_id, get_user_by_name},
};
use super::HandlerResult;

#[derive(Deserialize)]
struct Move {
    actor: String,
    object: String,
    target: String,
}

pub async fn handle_move(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Value,
) -> HandlerResult {
    // Move(Person)
    let activity: Move = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    // Mastodon: actor is old profile (object)
    // Mitra: actor is new profile (target)
    if activity.object != activity.actor && activity.target != activity.actor {
        return Err(ValidationError("actor ID mismatch").into());
    };

    let instance = config.instance();
    let media_dir = config.media_dir();
    let old_profile = if let Ok(username) = parse_local_actor_id(
        &instance.url(),
        &activity.object,
    ) {
        let old_user = get_user_by_name(db_client, &username).await?;
        old_user.profile
    } else {
        get_or_import_profile_by_actor_id(
            db_client,
            &instance,
            &media_dir,
            &activity.object,
        ).await?
    };
    let old_actor_id = old_profile.actor_id(&instance.url());
    let new_profile = get_or_import_profile_by_actor_id(
        db_client,
        &instance,
        &media_dir,
        &activity.target,
    ).await?;
    let new_actor = new_profile.actor_json.as_ref()
        .expect("target should be a remote actor");

    // Find aliases by DIDs
    let mut aliases = find_aliases(db_client, &new_profile).await?
        .into_iter()
        .map(|profile| profile.actor_id(&instance.url()))
        .collect::<Vec<_>>();
    // Read aliases from alsoKnownAs property
    if let Some(ref value) = new_actor.also_known_as {
        let also_known_as = parse_array(value)
            .map_err(|_| ValidationError("invalid alias list"))?;
        aliases.extend(also_known_as);
    };
    if !aliases.contains(&old_actor_id) {
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
        // Send Undo(Follow) if old actor is not local
        if let Some(ref old_actor) = old_profile.actor_json {
            let follow_request_id = maybe_follow_request_id
                .expect("follow request must exist");
            activities.push(prepare_undo_follow(
                &instance,
                &follower,
                old_actor,
                &follow_request_id,
            ));
        };
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
                    new_actor,
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