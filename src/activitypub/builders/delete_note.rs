use serde_json::json;
use tokio_postgres::GenericClient;

use crate::activitypub::activity::{create_activity, Activity, Object};
use crate::activitypub::constants::{AP_CONTEXT, AP_PUBLIC};
use crate::activitypub::deliverer::OutgoingActivity;
use crate::activitypub::vocabulary::{DELETE, NOTE, TOMBSTONE};
use crate::config::Instance;
use crate::errors::DatabaseError;
use crate::mastodon_api::statuses::helpers::get_note_recipients;
use crate::models::posts::types::Post;
use crate::models::users::types::User;

fn build_delete_note(
    instance_url: &str,
    post: &Post,
) -> Activity {
    let object_id = post.get_object_id(instance_url);
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: object_id,
        object_type: TOMBSTONE.to_string(),
        former_type: Some(NOTE.to_string()),
        ..Default::default()
    };
    let activity_id = format!("{}/delete", object.id);
    let activity = create_activity(
        instance_url,
        &post.author.username,
        DELETE,
        activity_id,
        object,
        vec![AP_PUBLIC.to_string()],
        vec![],
    );
    activity
}

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
