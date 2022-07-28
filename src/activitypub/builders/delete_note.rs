use serde_json::json;
use tokio_postgres::GenericClient;

use crate::activitypub::activity::{create_activity, Activity, Object};
use crate::activitypub::constants::AP_CONTEXT;
use crate::activitypub::deliverer::OutgoingActivity;
use crate::activitypub::vocabulary::{DELETE, NOTE, TOMBSTONE};
use crate::config::Instance;
use crate::errors::DatabaseError;
use crate::models::posts::types::{Post, Visibility};
use crate::models::profiles::types::DbActorProfile;
use crate::models::relationships::queries::get_subscribers;
use crate::models::users::types::User;
use super::create_note::{
    build_note,
    get_note_recipients,
    Note,
};

fn build_delete_note(
    instance_host: &str,
    instance_url: &str,
    post: &Post,
    subscribers: Vec<DbActorProfile>,
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
    let Note { to, cc, .. } = build_note(
        instance_host,
        instance_url,
        post,
        subscribers,
    );
    let activity = create_activity(
        instance_url,
        &post.author.username,
        DELETE,
        activity_id,
        object,
        to,
        cc,
    );
    activity
}

pub async fn prepare_delete_note(
    db_client: &impl GenericClient,
    instance: Instance,
    author: &User,
    post: &Post,
) -> Result<OutgoingActivity<Activity>, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let subscribers = if matches!(post.visibility, Visibility::Subscribers) {
        get_subscribers(db_client, &author.id).await?
    } else {
        vec![]
    };
    let activity = build_delete_note(
        &instance.host(),
        &instance.url(),
        post,
        subscribers,
    );
    let recipients = get_note_recipients(db_client, author, post).await?;
    Ok(OutgoingActivity {
        instance,
        sender: author.clone(),
        activity,
        recipients,
    })
}
