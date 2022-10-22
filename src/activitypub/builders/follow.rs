use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity},
    actors::types::Actor,
    deliverer::OutgoingActivity,
    identifiers::local_object_id,
    vocabulary::FOLLOW,
};
use crate::config::Instance;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;

fn build_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    target_actor_id: &str,
    follow_request_id: &Uuid,
) -> Activity {
    let object = target_actor_id.to_string();
    let activity_id = local_object_id(instance_url, follow_request_id);
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        FOLLOW,
        activity_id,
        object,
        vec![target_actor_id.to_string()],
        vec![],
    );
    activity
}

pub fn prepare_follow(
    instance: &Instance,
    sender: &User,
    target_actor: &Actor,
    follow_request_id: &Uuid,
) -> OutgoingActivity<Activity> {
    let activity = build_follow(
        &instance.url(),
        &sender.profile,
        &target_actor.id,
        follow_request_id,
    );
    let recipients = vec![target_actor.clone()];
    OutgoingActivity {
        instance: instance.clone(),
        sender: sender.clone(),
        activity,
        recipients,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::utils::id::new_uuid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_follow() {
        let follower = DbActorProfile {
            username: "follower".to_string(),
            ..Default::default()
        };
        let follow_request_id = new_uuid();
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
        assert_eq!(activity.to.unwrap(), json!([target_actor_id]));
        assert_eq!(activity.cc.unwrap(), json!([]));
    }
}
