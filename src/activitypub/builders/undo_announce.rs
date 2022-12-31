use serde::Serialize;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    constants::{AP_CONTEXT, AP_PUBLIC},
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_actor_followers, local_object_id},
    vocabulary::UNDO,
};
use crate::config::Instance;
use crate::database::DatabaseError;
use crate::models::posts::types::Post;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;
use super::announce::get_announce_recipients;

#[derive(Serialize)]
struct UndoAnnounce {
    #[serde(rename = "@context")]
    context: String,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_undo_announce(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    repost_id: &Uuid,
    recipient_id: &str,
) -> UndoAnnounce {
    let object_id = local_object_id(instance_url, repost_id);
    let activity_id = format!("{}/undo", object_id);
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    let primary_audience = vec![
        AP_PUBLIC.to_string(),
        recipient_id.to_string(),
    ];
    let secondary_audience = vec![
        local_actor_followers(instance_url, &actor_profile.username),
    ];
    UndoAnnounce {
        context: AP_CONTEXT.to_string(),
        activity_type: UNDO.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object_id,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn prepare_undo_announce(
    db_client: &impl GenericClient,
    instance: &Instance,
    sender: &User,
    post: &Post,
    repost_id: &Uuid,
) -> Result<OutgoingActivity, DatabaseError> {
    assert_ne!(&post.id, repost_id);
    let (recipients, primary_recipient) = get_announce_recipients(
        db_client,
        &instance.url(),
        sender,
        post,
    ).await?;
    let activity = build_undo_announce(
        &instance.url(),
        &sender.profile,
        repost_id,
        &primary_recipient,
    );
    Ok(OutgoingActivity::new(
        instance,
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use crate::utils::id::new_uuid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_undo_announce() {
        let announcer = DbActorProfile::default();
        let post_author_id = "https://example.com/users/test";
        let repost_id = new_uuid();
        let activity = build_undo_announce(
            INSTANCE_URL,
            &announcer,
            &repost_id,
            post_author_id,
        );
        assert_eq!(
            activity.id,
            format!("{}/objects/{}/undo", INSTANCE_URL, repost_id),
        );
        assert_eq!(
            activity.object,
            format!("{}/objects/{}", INSTANCE_URL, repost_id),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC, post_author_id]);
    }
}