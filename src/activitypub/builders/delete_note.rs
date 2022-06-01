use tokio_postgres::GenericClient;

use crate::activitypub::activity::create_activity_delete_note as build_delete_note;
use crate::activitypub::deliverer::OutgoingActivity;
use crate::config::Instance;
use crate::errors::DatabaseError;
use crate::mastodon_api::statuses::helpers::get_note_recipients;
use crate::models::posts::types::Post;
use crate::models::users::types::User;

pub async fn prepare_delete_note(
    db_client: &impl GenericClient,
    instance: Instance,
    author: &User,
    post: &Post,
) -> Result<OutgoingActivity, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let activity = build_delete_note(&instance.url(), post);
    let recipients = get_note_recipients(db_client, author, post).await?;
    Ok(OutgoingActivity {
        instance,
        sender: author.clone(),
        activity,
        recipients,
    })
}
