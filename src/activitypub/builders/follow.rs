use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;

use crate::activitypub::{
    actors::types::Actor,
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id},
    types::{build_default_context, Context},
    vocabulary::FOLLOW,
};
use crate::models::{
    profiles::types::DbActorProfile,
    users::types::User,
};

#[derive(Serialize)]
pub(super) struct Follow {
    #[serde(rename = "@context")]
    pub context: Context,

    #[serde(rename = "type")]
    pub activity_type: String,

    pub id: String,
    pub actor: String,
    pub object: String,

    pub to: Vec<String>,
}

fn build_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    target_actor_id: &str,
    follow_request_id: &Uuid,
) -> Follow {
    let activity_id = local_object_id(instance_url, follow_request_id);
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    Follow {
        context: build_default_context(),
        activity_type: FOLLOW.to_string(),
        id: activity_id,
        actor: actor_id,
        object: target_actor_id.to_string(),
        to: vec![target_actor_id.to_string()],
    }
}

pub fn prepare_follow(
    instance: &Instance,
    sender: &User,
    target_actor: &Actor,
    follow_request_id: &Uuid,
) -> OutgoingActivity {
    let activity = build_follow(
        &instance.url(),
        &sender.profile,
        &target_actor.id,
        follow_request_id,
    );
    let recipients = vec![target_actor.clone()];
    OutgoingActivity::new(
        instance,
        sender,
        activity,
        recipients,
    )
}

#[cfg(test)]
mod tests {
    use mitra_utils::id::generate_ulid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_follow() {
        let follower = DbActorProfile {
            username: "follower".to_string(),
            ..Default::default()
        };
        let follow_request_id = generate_ulid();
        let target_actor_id = "https://test.remote/actor/test";
        let activity = build_follow(
            INSTANCE_URL,
            &follower,
            target_actor_id,
            &follow_request_id,
        );

        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, follow_request_id),
        );
        assert_eq!(activity.activity_type, "Follow");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URL, follower.username),
        );
        assert_eq!(activity.object, target_actor_id);
        assert_eq!(activity.to, vec![target_actor_id]);
    }
}
