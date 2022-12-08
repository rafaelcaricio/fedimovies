use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    receiver::find_object_id,
    vocabulary::{ANNOUNCE, FOLLOW, LIKE},
};
use crate::config::Config;
use crate::database::DatabaseError;
use crate::errors::ValidationError;
use crate::models::posts::queries::{
    delete_post,
    get_post_by_remote_object_id,
};
use crate::models::profiles::queries::get_profile_by_remote_actor_id;
use crate::models::reactions::queries::{
    delete_reaction,
    get_reaction_by_remote_activity_id,
};
use super::HandlerResult;
use super::undo_follow::handle_undo_follow;

pub async fn handle_undo(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    if let Some(FOLLOW) = activity.object["type"].as_str() {
        // Object type is currently required for processing Undo(Follow)
        // because activity IDs of remote follow requests are not stored.
        return handle_undo_follow(config, db_client, activity).await
    };

    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let object_id = find_object_id(&activity.object)?;
    match get_reaction_by_remote_activity_id(db_client, &object_id).await {
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
                &object_id,
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
