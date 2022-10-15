use chrono::Utc;
use tokio_postgres::GenericClient;

use crate::activitypub::activity::Object;
use crate::activitypub::vocabulary::NOTE;
use crate::errors::DatabaseError;
use crate::models::posts::queries::{
    get_post_by_remote_object_id,
    update_post,
};
use crate::models::posts::types::PostUpdateData;
use super::HandlerResult;
use super::create_note::get_note_content;

pub async fn handle_update_note(
    db_client: &mut impl GenericClient,
    object: Object,
) -> HandlerResult {
    let post_id = match get_post_by_remote_object_id(
        db_client,
        &object.id,
    ).await {
        Ok(post) => post.id,
        // Ignore Update if post is not found locally
        Err(DatabaseError::NotFound(_)) => return Ok(None),
        Err(other_error) => return Err(other_error.into()),
    };
    let content = get_note_content(&object)?;
    let updated_at = object.updated.unwrap_or(Utc::now());
    let post_data = PostUpdateData { content, updated_at };
    update_post(db_client, &post_id, post_data).await?;
    Ok(Some(NOTE))
}
