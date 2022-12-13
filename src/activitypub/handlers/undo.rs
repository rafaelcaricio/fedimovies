use serde::Deserialize;
use serde_json::Value;
use tokio_postgres::GenericClient;

use crate::activitypub::{
    identifiers::parse_local_actor_id,
    receiver::{deserialize_into_object_id, find_object_id},
    vocabulary::{ANNOUNCE, FOLLOW, LIKE},
};
use crate::config::Config;
use crate::database::DatabaseError;
use crate::errors::ValidationError;
use crate::models::{
    posts::queries::{
        delete_post,
        get_post_by_remote_object_id,
    },
    profiles::queries::{
        get_profile_by_acct,
        get_profile_by_remote_actor_id,
    },
    reactions::queries::{
        delete_reaction,
        get_reaction_by_remote_activity_id,
    },
    relationships::queries::{
        get_follow_request_by_activity_id,
        unfollow,
    },
};
use super::HandlerResult;

#[derive(Deserialize)]
struct UndoFollow {
    actor: String,
    object: Value,
}

async fn handle_undo_follow(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Value,
) -> HandlerResult {
    let activity: UndoFollow = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
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

#[derive(Deserialize)]
struct Undo {
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_undo(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Value,
) -> HandlerResult {
    if let Some(FOLLOW) = activity["object"]["type"].as_str() {
        // Undo() with nested follow activity
        return handle_undo_follow(config, db_client, activity).await;
    };

    let activity: Undo = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;

    match get_follow_request_by_activity_id(db_client, &activity.object).await {
        Ok(follow_request) => {
            // Undo(Follow)
            unfollow(
                db_client,
                &follow_request.source_id,
                &follow_request.target_id,
            ).await?;
            return Ok(Some(FOLLOW));
        },
        Err(DatabaseError::NotFound(_)) => (), // try other object types
        Err(other_error) => return Err(other_error.into()),
    };

    match get_reaction_by_remote_activity_id(db_client, &activity.object).await {
        Ok(reaction) => {
            // Undo(Like)
            if reaction.author_id != actor_profile.id {
                return Err(ValidationError("actor is not an author").into());
            };
            delete_reaction(
                db_client,
                &reaction.author_id,
                &reaction.post_id,
            ).await?;
            Ok(Some(LIKE))
        },
        Err(DatabaseError::NotFound(_)) => {
            // Undo(Announce)
            let post = match get_post_by_remote_object_id(
                db_client,
                &activity.object,
            ).await {
                Ok(post) => post,
                // Ignore undo if neither reaction nor repost is found
                Err(DatabaseError::NotFound(_)) => return Ok(None),
                Err(other_error) => return Err(other_error.into()),
            };
            if post.author.id != actor_profile.id {
                return Err(ValidationError("actor is not an author").into());
            };
            match post.repost_of_id {
                // Ignore returned data because reposts don't have attached files
                Some(_) => delete_post(db_client, &post.id).await?,
                // Can't undo regular post
                None => return Err(ValidationError("object is not a repost").into()),
            };
            Ok(Some(ANNOUNCE))
        },
        Err(other_error) => Err(other_error.into()),
    }
}
