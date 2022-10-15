use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    fetcher::helpers::{get_or_import_profile_by_actor_id, import_post},
    identifiers::parse_local_object_id,
    receiver::find_object_id,
    vocabulary::{CREATE, LIKE, NOTE, UPDATE},
};
use crate::config::Config;
use crate::errors::DatabaseError;
use crate::models::posts::queries::{
    create_post,
    get_post_by_remote_object_id,
};
use crate::models::posts::types::PostCreateData;
use super::HandlerResult;

pub async fn handle_announce(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Activity,
) -> HandlerResult {
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
    if let Some(CREATE) | Some(LIKE) | Some(UPDATE) = activity.object["type"].as_str() {
        // Ignore Announce(Create) activities from Lemmy
        return Ok(None);
    };
    let object_id = find_object_id(&activity.object)?;
    let post_id = match parse_local_object_id(&config.instance_url(), &object_id) {
        Ok(post_id) => post_id,
        Err(_) => {
            // Try to get remote post
            let post = import_post(config, db_client, object_id, None).await?;
            post.id
        },
    };
    let repost_data = PostCreateData::repost(post_id, Some(repost_object_id));
    create_post(db_client, &author.id, repost_data).await?;
    Ok(Some(NOTE))
}
