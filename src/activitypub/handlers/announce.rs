use serde::Deserialize;
use serde_json::Value;

use fedimovies_config::Config;
use fedimovies_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::{create_post, get_post_by_remote_object_id},
    posts::types::PostCreateData,
};

use super::HandlerResult;
use crate::activitypub::{
    fetcher::helpers::{get_or_import_profile_by_actor_id, import_post},
    identifiers::parse_local_object_id,
    receiver::deserialize_into_object_id,
    vocabulary::{CREATE, DELETE, DISLIKE, LIKE, NOTE, UNDO, UPDATE},
};
use crate::errors::ValidationError;
use crate::media::MediaStorage;

#[derive(Deserialize)]
struct Announce {
    id: String,
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_announce(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    if let Some(CREATE | DELETE | DISLIKE | LIKE | UNDO | UPDATE) =
        activity["object"]["type"].as_str()
    {
        // Ignore wrapped activities from Lemmy
        // https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-1b12.md
        return Ok(None);
    };
    let activity: Announce = serde_json::from_value(activity.clone())
        .map_err(|_| ValidationError(format!("unexpected activity structure: {}", activity)))?;
    let repost_object_id = activity.id;
    match get_post_by_remote_object_id(db_client, &repost_object_id).await {
        Ok(_) => return Ok(None), // Ignore if repost already exists
        Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    let instance = config.instance();
    let storage = MediaStorage::from(config);
    let author =
        get_or_import_profile_by_actor_id(db_client, &instance, &storage, &activity.actor).await?;
    let post_id = match parse_local_object_id(&instance.url(), &activity.object) {
        Ok(post_id) => post_id,
        Err(_) => {
            // Try to get remote post
            let tmdb_api_key = config.tmdb_api_key.clone();
            let default_movie_user_password = config.movie_user_password.clone();
            let post = import_post(
                db_client,
                &instance,
                &storage,
                tmdb_api_key,
                default_movie_user_password,
                activity.object,
                None,
            )
            .await?;
            post.id
        }
    };
    let repost_data = PostCreateData::repost(post_id, Some(repost_object_id.clone()));
    match create_post(db_client, &author.id, repost_data).await {
        Ok(_) => Ok(Some(NOTE)),
        Err(DatabaseError::AlreadyExists("post")) => {
            // Ignore activity if repost already exists (with a different
            // object ID, or due to race condition in a handler).
            log::warn!("repost already exists: {}", repost_object_id);
            Ok(None)
        }
        // May return "post not found" error if post if not public
        Err(other_error) => Err(other_error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deserialize_announce() {
        let activity_raw = json!({
            "type": "Announce",
            "id": "https://example.com/activities/321",
            "actor": "https://example.com/users/1",
            "object": "https://test.org/objects/999",
        });
        let activity: Announce = serde_json::from_value(activity_raw).unwrap();
        assert_eq!(activity.object, "https://test.org/objects/999");
    }

    #[test]
    fn test_deserialize_announce_nested() {
        let activity_raw = json!({
            "type": "Announce",
            "id": "https://example.com/activities/321",
            "actor": "https://example.com/users/1",
            "object": {
                "type": "Note",
                "id": "https://test.org/objects/999",
            },
        });
        let activity: Announce = serde_json::from_value(activity_raw).unwrap();
        assert_eq!(activity.object, "https://test.org/objects/999");
    }
}
