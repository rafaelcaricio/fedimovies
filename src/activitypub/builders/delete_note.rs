use serde_json::json;
use tokio_postgres::GenericClient;

use crate::activitypub::activity::{create_activity, Activity, Object};
use crate::activitypub::constants::AP_CONTEXT;
use crate::activitypub::deliverer::OutgoingActivity;
use crate::activitypub::vocabulary::{DELETE, NOTE, TOMBSTONE};
use crate::config::Instance;
use crate::database::DatabaseError;
use crate::models::posts::helpers::add_related_posts;
use crate::models::posts::types::Post;
use crate::models::users::types::User;
use super::create_note::{
    build_note,
    get_note_recipients,
    Note,
};

fn build_delete_note(
    instance_hostname: &str,
    instance_url: &str,
    post: &Post,
) -> Activity {
    let object_id = post.object_id(instance_url);
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: object_id,
        object_type: TOMBSTONE.to_string(),
        former_type: Some(NOTE.to_string()),
        ..Default::default()
    };
    let activity_id = format!("{}/delete", object.id);
    let Note { to, cc, .. } = build_note(
        instance_hostname,
        instance_url,
        post,
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
    instance: &Instance,
    author: &User,
    post: &Post,
) -> Result<OutgoingActivity<Activity>, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let mut post = post.clone();
    add_related_posts(db_client, vec![&mut post]).await?;
    let activity = build_delete_note(
        &instance.hostname(),
        &instance.url(),
        &post,
    );
    let recipients = get_note_recipients(db_client, author, &post).await?;
    Ok(OutgoingActivity {
        instance: instance.clone(),
        sender: author.clone(),
        activity,
        recipients,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::activitypub::{
        constants::AP_PUBLIC,
        identifiers::local_actor_followers,
    };
    use crate::models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_HOSTNAME: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_delete_note() {
        let author = DbActorProfile {
            username: "author".to_string(),
            ..Default::default()
        };
        let post = Post { author, ..Default::default() };
        let activity = build_delete_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &post,
        );

        assert_eq!(
            activity.id,
            format!("{}/objects/{}/delete", INSTANCE_URL, post.id),
        );
        assert_eq!(
            activity.object["id"],
            format!("{}/objects/{}", INSTANCE_URL, post.id),
        );
        assert_eq!(activity.to.unwrap(), json!([AP_PUBLIC]));
        assert_eq!(
            activity.cc.unwrap(),
            json!([local_actor_followers(INSTANCE_URL, "author")]),
        );
    }
}
