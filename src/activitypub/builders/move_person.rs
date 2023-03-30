use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    profiles::types::DbActor,
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::activitypub::{
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id},
    types::{build_default_context, Context},
    vocabulary::MOVE,
};

#[derive(Serialize)]
pub struct MovePerson {
    #[serde(rename = "@context")]
    context: Context,

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
    maybe_internal_activity_id: Option<&Uuid>,
) -> MovePerson {
    let internal_activity_id = maybe_internal_activity_id.copied()
        .unwrap_or(generate_ulid());
    let activity_id = local_object_id(instance_url, &internal_activity_id);
    let actor_id = local_actor_id(instance_url, &sender.profile.username);
    MovePerson {
        context: build_default_context(),
        activity_type: MOVE.to_string(),
        id: activity_id,
        actor: actor_id.clone(),
        object: from_actor_id.to_string(),
        target: actor_id,
        to: followers.to_vec(),
    }
}

pub fn prepare_move_person(
    instance: &Instance,
    sender: &User,
    from_actor_id: &str,
    followers: Vec<DbActor>,
    maybe_internal_activity_id: Option<&Uuid>,
) -> OutgoingActivity {
    let followers_ids: Vec<String> = followers.iter()
        .map(|actor| actor.id.clone())
        .collect();
    let activity = build_move_person(
        &instance.url(),
        sender,
        from_actor_id,
        &followers_ids,
        maybe_internal_activity_id,
    );
    OutgoingActivity::new(
        instance,
        sender,
        activity,
        followers,
    )
}

#[cfg(test)]
mod tests {
    use mitra_utils::id::generate_ulid;
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_move_person() {
        let sender = User {
            profile: DbActorProfile {
                username: "testuser".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let from_actor_id = "https://server0.org/users/test";
        let followers = vec![
            "https://server1.org/users/1".to_string(),
            "https://server2.org/users/2".to_string(),
        ];
        let internal_activity_id = generate_ulid();
        let activity = build_move_person(
            INSTANCE_URL,
            &sender,
            from_actor_id,
            &followers,
            Some(&internal_activity_id),
        );

        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, internal_activity_id),
        );
        assert_eq!(activity.activity_type, "Move");
        assert_eq!(
            activity.actor,
            format!("{}/users/{}", INSTANCE_URL, sender.profile.username),
        );
        assert_eq!(activity.object, from_actor_id);
        assert_eq!(activity.target, activity.actor);
        assert_eq!(activity.to, followers);
    }
}
