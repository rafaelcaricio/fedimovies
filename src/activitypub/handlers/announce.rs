use serde::Deserialize;
use serde_json::Value;
use tokio_postgres::GenericClient;

use crate::activitypub::{
    fetcher::helpers::{get_or_import_profile_by_actor_id, import_post},
    identifiers::parse_local_object_id,
    receiver::deserialize_into_object_id,
    vocabulary::{CREATE, LIKE, NOTE, UNDO, UPDATE},
};
use crate::config::Config;
use crate::database::DatabaseError;
use crate::errors::ValidationError;
use crate::models::posts::queries::{
    create_post,
    get_post_by_remote_object_id,
};
use crate::models::posts::types::PostCreateData;
use super::HandlerResult;

#[derive(Deserialize)]
struct Announce {
    id: String,
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_announce(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Value,
) -> HandlerResult {
    if let Some(CREATE | LIKE | UNDO | UPDATE) = activity["object"]["type"].as_str() {
        // Ignore wrapped activities from Lemmy
        // https://codeberg.org/fediverse/fep/src/branch/main/feps/fep-1b12.md
        return Ok(None);
    };
    let activity: Announce = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let repost_object_id = activity.id;
    match get_post_by_remote_object_id(
        db_client,
        &repost_object_id,
    ).await {
        Ok(_) => return Ok(None), // Ignore if repost already exists
        Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    let author = get_or_import_profile_by_actor_id(
        db_client,
        &config.instance(),
        &config.media_dir(),
        &activity.actor,
    ).await?;
    let post_id = match parse_local_object_id(
        &config.instance_url(),
        &activity.object,
    ) {
        Ok(post_id) => post_id,
        Err(_) => {
            // Try to get remote post
            let post = import_post(config, db_client, activity.object, None).await?;
            post.id
        },
    };
    let repost_data = PostCreateData::repost(post_id, Some(repost_object_id));
    match create_post(db_client, &author.id, repost_data).await {
        Ok(_) => Ok(Some(NOTE)),
        Err(DatabaseError::AlreadyExists("post")) => {
            // Ignore activity if repost already exists (with a different
            // object ID, or due to race condition in a handler).
            log::warn!("repost already exists");
            Ok(None)
        },
        Err(other_error) => Err(other_error.into()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

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
