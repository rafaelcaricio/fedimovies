use serde::Serialize;

use crate::activitypub::{
    actors::types::Actor,
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id},
    types::{build_default_context, Context},
    vocabulary::ACCEPT,
};
use crate::config::Instance;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;
use crate::utils::id::generate_ulid;

#[derive(Serialize)]
struct AcceptFollow {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,

    to: Vec<String>,
}

fn build_accept_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    source_actor_id: &str,
    follow_activity_id: &str,
) -> AcceptFollow {
    // Accept(Follow) is idempotent so its ID can be random
    let activity_id = local_object_id(instance_url, &generate_ulid());
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    AcceptFollow {
        context: build_default_context(),
        activity_type: ACCEPT.to_string(),
        id: activity_id,
        actor: actor_id,
        object: follow_activity_id.to_string(),
        to: vec![source_actor_id.to_string()],
    }
}

pub fn prepare_accept_follow(
    instance: &Instance,
    sender: &User,
    source_actor: &Actor,
    follow_activity_id: &str,
) -> OutgoingActivity {
    let activity = build_accept_follow(
        &instance.url(),
        &sender.profile,
        &source_actor.id,
        follow_activity_id,
    );
    let recipients = vec![source_actor.clone()];
    OutgoingActivity::new(
        instance,
        sender,
        activity,
        recipients,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_accept_follow() {
        let target = DbActorProfile {
            username: "user".to_string(),
            ..Default::default()
        };
        let follow_activity_id = "https://test.remote/objects/999";
        let follower_id = "https://test.remote/users/123";
        let activity = build_accept_follow(
            INSTANCE_URL,
            &target,
            follower_id,
            follow_activity_id,
        );

        assert_eq!(activity.id.starts_with(INSTANCE_URL), true);
        assert_eq!(activity.activity_type, "Accept");
        assert_eq!(activity.object, follow_activity_id);
        assert_eq!(activity.to, vec![follower_id]);
    }
}
