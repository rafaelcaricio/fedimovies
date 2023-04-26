use serde::Deserialize;
use serde_json::Value;

use fedimovies_config::Config;
use fedimovies_models::{
    database::DatabaseClient,
    notifications::queries::create_move_notification,
    profiles::helpers::find_verified_aliases,
    relationships::queries::{get_followers, unfollow},
    users::queries::{get_user_by_id, get_user_by_name},
};

use crate::activitypub::{
    builders::{follow::follow_or_create_request, undo_follow::prepare_undo_follow},
    fetcher::helpers::get_or_import_profile_by_actor_id,
    identifiers::{parse_local_actor_id, profile_actor_id},
    vocabulary::PERSON,
};
use crate::errors::ValidationError;
use crate::media::MediaStorage;

use super::HandlerResult;

#[derive(Deserialize)]
struct Move {
    actor: String,
    object: String,
    target: String,
}

pub async fn handle_move(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    // Move(Person)
    let activity: Move = serde_json::from_value(activity.clone()).map_err(|_| {
        ValidationError(format!("unexpected Move activity structure: {}", activity))
    })?;
    // Mastodon: actor is old profile (object)
    // Mitra: actor is new profile (target)
    if activity.object != activity.actor && activity.target != activity.actor {
        return Err(ValidationError("actor ID mismatch".to_string()).into());
    };

    let instance = config.instance();
    let storage = MediaStorage::from(config);
    let old_profile = if let Ok(username) = parse_local_actor_id(&instance.url(), &activity.object)
    {
        let old_user = get_user_by_name(db_client, &username).await?;
        old_user.profile
    } else {
        get_or_import_profile_by_actor_id(db_client, &instance, &storage, &activity.object).await?
    };
    let old_actor_id = profile_actor_id(&instance.url(), &old_profile);

    let new_profile = if let Ok(username) = parse_local_actor_id(&instance.url(), &activity.target)
    {
        let new_user = get_user_by_name(db_client, &username).await?;
        new_user.profile
    } else {
        get_or_import_profile_by_actor_id(db_client, &instance, &storage, &activity.target).await?
    };

    // Find aliases by DIDs (verified)
    let mut aliases = find_verified_aliases(db_client, &new_profile)
        .await?
        .into_iter()
        .map(|profile| profile_actor_id(&instance.url(), &profile))
        .collect::<Vec<_>>();
    // Add aliases reported by server (actor's alsoKnownAs property)
    aliases.extend(new_profile.aliases.clone().into_actor_ids());
    if !aliases.contains(&old_actor_id) {
        return Err(ValidationError("target ID is not an alias".to_string()).into());
    };

    let followers = get_followers(db_client, &old_profile.id).await?;
    for follower in followers {
        let follower = get_user_by_id(db_client, &follower.id).await?;
        // Unfollow old profile
        let maybe_follow_request_id = unfollow(db_client, &follower.id, &old_profile.id).await?;
        // Send Undo(Follow) if old actor is not local
        if let Some(ref old_actor) = old_profile.actor_json {
            let follow_request_id = maybe_follow_request_id.expect("follow request must exist");
            prepare_undo_follow(&instance, &follower, old_actor, &follow_request_id)
                .enqueue(db_client)
                .await?;
        };
        if follower.id == new_profile.id {
            // Don't self-follow
            continue;
        };
        // Follow new profile
        follow_or_create_request(db_client, &instance, &follower, &new_profile).await?;
        create_move_notification(db_client, &new_profile.id, &follower.id).await?;
    }

    Ok(Some(PERSON))
}
