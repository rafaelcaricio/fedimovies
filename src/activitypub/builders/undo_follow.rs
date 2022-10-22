use serde_json::json;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity, Object},
    actors::types::Actor,
    constants::AP_CONTEXT,
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id},
    vocabulary::{FOLLOW, UNDO},
};
use crate::config::Instance;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;

fn build_undo_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    target_actor_id: &str,
    follow_request_id: &Uuid,
) -> Activity {
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
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        UNDO,
        activity_id,
        object,
        vec![target_actor_id.to_string()],
        vec![],
    );
    activity
}

pub fn prepare_undo_follow(
    instance: &Instance,
    sender: &User,
    target_actor: &Actor,
    follow_request_id: &Uuid,
) -> OutgoingActivity<Activity> {
    let activity = build_undo_follow(
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
