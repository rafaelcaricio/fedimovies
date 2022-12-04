use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::activitypub::{
    activity::Object,
    actors::types::Actor,
    constants::AP_CONTEXT,
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id},
    vocabulary::{FOLLOW, UNDO},
};
use crate::config::Instance;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;

#[derive(Serialize)]
struct UndoFollow {
    #[serde(rename = "@context")]
    context: String,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Object,

    to: Vec<String>,
}

fn build_undo_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    target_actor_id: &str,
    follow_request_id: &Uuid,
) -> UndoFollow {
    let follow_activity_id = local_object_id(
        instance_url,
        follow_request_id,
    );
    let follow_actor_id = local_actor_id(
        instance_url,
        &actor_profile.username,
    );
    let object = Object {
        context: Some(json!(AP_CONTEXT)),
        id: follow_activity_id,
        object_type: FOLLOW.to_string(),
        actor: Some(follow_actor_id),
        object: Some(target_actor_id.to_owned()),
        ..Default::default()
    };
    let activity_id = format!("{}/undo", object.id);
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    UndoFollow {
        context: AP_CONTEXT.to_string(),
        activity_type: UNDO.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object,
        to: vec![target_actor_id.to_string()],
    }
}

pub fn prepare_undo_follow(
    instance: &Instance,
    sender: &User,
    target_actor: &Actor,
    follow_request_id: &Uuid,
) -> OutgoingActivity {
    let activity = build_undo_follow(
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
    use crate::utils::id::new_uuid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_undo_follow() {
        let actor_profile = DbActorProfile {
            username: "user".to_string(),
            ..Default::default()
        };
        let target_actor_id = "https://test.remote/users/123";
        let follow_request_id = new_uuid();
        let activity = build_undo_follow(
            INSTANCE_URL,
            &actor_profile,
            target_actor_id,
            &follow_request_id,
        );

        assert_eq!(
            activity.id,
            format!("{}/objects/{}/undo", INSTANCE_URL, follow_request_id),
        );
        assert_eq!(activity.activity_type, "Undo");
        assert_eq!(
            activity.object.id,
            format!("{}/objects/{}", INSTANCE_URL, follow_request_id),
        );
        assert_eq!(activity.object.object.unwrap(), target_actor_id);
        assert_eq!(activity.to, vec![target_actor_id]);
    }
}
