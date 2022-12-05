use serde_json::json;

use crate::activitypub::{
    activity::{create_activity, Activity, Object},
    actors::types::Actor,
    constants::AP_CONTEXT,
    deliverer::OutgoingActivity,
    identifiers::local_object_id,
    vocabulary::{ACCEPT, FOLLOW},
};
use crate::config::Instance;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;
use crate::utils::id::new_uuid;

fn build_accept_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    source_actor_id: &str,
    follow_activity_id: &str,
) -> Activity {
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: follow_activity_id.to_string(),
        object_type: FOLLOW.to_string(),
        ..Default::default()
    };
    // Accept(Follow) is idempotent so its ID can be random
    let activity_id = local_object_id(instance_url, &new_uuid());
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        ACCEPT,
        activity_id,
        object,
        vec![source_actor_id.to_string()],
        vec![],
    );
    activity
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
        assert_eq!(activity.object["id"], follow_activity_id);
        assert_eq!(activity.to.unwrap(), json!([follower_id]));
    }
}
