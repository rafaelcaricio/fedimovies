use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::activitypub::{
    actors::types::Actor,
    constants::AP_CONTEXT,
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id},
    vocabulary::MOVE,
};
use crate::config::Instance;
use crate::errors::ConversionError;
use crate::models::users::types::User;

#[derive(Serialize)]
pub struct MovePerson {
    #[serde(rename = "@context")]
    context: String,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,
    target: String,

    to: Vec<String>,
}

pub fn build_move_person(
    instance_url: &str,
    sender: &User,
    from_actor_id: &str,
    followers: &[String],
    internal_activity_id: &Uuid,
) -> MovePerson {
    let activity_id = local_object_id(instance_url, internal_activity_id);
    let actor_id = local_actor_id(instance_url, &sender.profile.username);
    MovePerson {
        context: AP_CONTEXT.to_string(),
        activity_type: MOVE.to_string(),
        id: activity_id,
        actor: actor_id.clone(),
        object: from_actor_id.to_string(),
        target: actor_id,
        to: followers.to_vec(),
    }
}

pub fn prepare_signed_move_person(
    instance: &Instance,
    sender: &User,
    from_actor_id: &str,
    followers: Vec<Actor>,
    internal_activity_id: &Uuid,
) -> Result<OutgoingActivity<Value>, ConversionError> {
    let followers_ids: Vec<String> = followers.iter()
        .map(|actor| actor.id.clone())
        .collect();
    let activity = build_move_person(
        &instance.url(),
        sender,
        from_actor_id,
        &followers_ids,
        internal_activity_id,
    );
    let activity_value = serde_json::to_value(activity)
        .map_err(|_| ConversionError)?;
    Ok(OutgoingActivity {
        instance: instance.clone(),
        sender: sender.clone(),
        activity: activity_value,
        recipients: followers,
    })
}
