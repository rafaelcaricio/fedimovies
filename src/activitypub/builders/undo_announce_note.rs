use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity},
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    views::{get_followers_url, get_object_url},
    vocabulary::UNDO,
};
use crate::config::Instance;
use crate::errors::DatabaseError;
use crate::mastodon_api::statuses::helpers::{get_announce_recipients, Audience};
use crate::models::posts::types::Post;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;

fn build_undo_announce(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    repost_id: &Uuid,
    recipient_id: &str,
) -> Activity {
    let object_id = get_object_url(
        instance_url,
        repost_id,
    );
    let activity_id = format!("{}/undo", object_id);
    let primary_audience = vec![
        AP_PUBLIC.to_string(),
        recipient_id.to_string(),
    ];
    create_activity(
        instance_url,
        &actor_profile.username,
        UNDO,
        activity_id,
        object_id,
        primary_audience,
        vec![get_followers_url(instance_url, &actor_profile.username)],
    )
}

pub async fn prepare_undo_announce_note(
    db_client: &impl GenericClient,
    instance: Instance,
    user: &User,
    post: &Post,
    repost_id: &Uuid,
) -> Result<OutgoingActivity, DatabaseError> {
    assert_ne!(&post.id, repost_id);
    let Audience { recipients, primary_recipient } =
        get_announce_recipients(db_client, &instance.url(), user, post).await?;
    let activity = build_undo_announce(
        &instance.url(),
        &user.profile,
        repost_id,
        &primary_recipient,
    );
    Ok(OutgoingActivity {
        instance,
        sender: user.clone(),
        activity,
        recipients,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
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
        assert_eq!(activity.to.unwrap(), json!([AP_PUBLIC, post_author_id]));
    }
}
