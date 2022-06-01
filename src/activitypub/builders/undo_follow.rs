use serde_json::json;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity, Object},
    actor::Actor,
    constants::AP_CONTEXT,
    deliverer::OutgoingActivity,
    views::{get_actor_url, get_object_url},
    vocabulary::{FOLLOW, UNDO},
};
use crate::config::Instance;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;

fn build_undo_follow(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    follow_request_id: &Uuid,
    target_actor_id: &str,
) -> Activity {
    let follow_activity_id = get_object_url(
        instance_url,
        follow_request_id,
    );
    let follow_actor_id = get_actor_url(
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
    instance: Instance,
    user: &User,
    target_actor: &Actor,
    follow_request_id: &Uuid,
) -> OutgoingActivity {
    let activity = build_undo_follow(
        &instance.url(),
        &user.profile,
        follow_request_id,
        &target_actor.id,
    );
    let recipients = vec![target_actor.clone()];
    OutgoingActivity {
        instance,
        sender: user.clone(),
        activity,
        recipients,
    }
}