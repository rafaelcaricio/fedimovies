use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity},
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    views::{get_followers_url, get_object_url},
    vocabulary::ANNOUNCE,
};
use crate::config::Instance;
use crate::errors::DatabaseError;
use crate::mastodon_api::statuses::helpers::{get_announce_recipients, Audience};
use crate::models::posts::types::Post;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;

fn build_announce_note(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    post: &Post,
    repost_id: &Uuid,
) -> Activity {
    let object_id = post.get_object_id(instance_url);
    let activity_id = get_object_url(instance_url, repost_id);
    let recipient_id = post.author.actor_id(instance_url);
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        ANNOUNCE,
        activity_id,
        object_id,
        vec![AP_PUBLIC.to_string(), recipient_id],
        vec![get_followers_url(instance_url, &actor_profile.username)],
    );
    activity
}

pub async fn prepare_announce_note(
    db_client: &impl GenericClient,
    instance: Instance,
    user: &User,
    post: &Post,
    repost_id: &Uuid,
) -> Result<OutgoingActivity, DatabaseError> {
    assert_ne!(&post.id, repost_id);
    let Audience { recipients, .. } =
        get_announce_recipients(db_client, &instance.url(), user, post).await?;
    let activity = build_announce_note(
        &instance.url(),
        &user.profile,
        post,
        repost_id,
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
    use crate::activitypub::actor::Actor;
    use crate::utils::id::new_uuid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_announce_note() {
        let post_author_id = "https://test.net/user/test";
        let post_author = DbActorProfile {
            actor_json: Some(Actor {
                id: post_author_id.to_string(),
                ..Default::default()
            }),
            actor_id: Some(post_author_id.to_string()),
            ..Default::default()
        };
        let post_id = "https://test.net/obj/123";
        let post = Post {
            author: post_author.clone(),
            object_id: Some(post_id.to_string()),
            ..Default::default()
        };
        let announcer = DbActorProfile::default();
        let repost_id = new_uuid();
        let activity = build_announce_note(
            INSTANCE_URL,
            &announcer,
            &post,
            &repost_id,
        );
        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, repost_id),
        );
        assert_eq!(activity.object, post_id);
        assert_eq!(activity.to.unwrap(), json!([AP_PUBLIC, post_author_id]));
    }
}
