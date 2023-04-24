use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError, DatabaseTypeError},
    profiles::types::DbActor,
    relationships::queries::get_followers,
    users::types::User,
};
use mitra_utils::id::generate_ulid;

use crate::activitypub::{
    actors::types::{get_local_actor, Actor, ActorKeyError},
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    identifiers::{local_actor_followers, local_object_id},
    types::{build_default_context, Context},
    vocabulary::UPDATE,
};

#[derive(Serialize)]
pub struct UpdatePerson {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Actor,

    to: Vec<String>,
}

pub fn build_update_person(
    instance_url: &str,
    user: &User,
    maybe_internal_activity_id: Option<Uuid>,
) -> Result<UpdatePerson, ActorKeyError> {
    let actor = get_local_actor(user, instance_url)?;
    // Update(Person) is idempotent so its ID can be random
    let internal_activity_id = maybe_internal_activity_id.unwrap_or(generate_ulid());
    let activity_id = local_object_id(instance_url, &internal_activity_id);
    let activity = UpdatePerson {
        context: build_default_context(),
        activity_type: UPDATE.to_string(),
        id: activity_id,
        actor: actor.id.clone(),
        object: actor,
        to: vec![
            AP_PUBLIC.to_string(),
            local_actor_followers(instance_url, &user.profile.username),
        ],
    };
    Ok(activity)
}

async fn get_update_person_recipients(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
) -> Result<Vec<DbActor>, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let mut recipients = vec![];
    for profile in followers {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    }
    Ok(recipients)
}

pub async fn prepare_update_person(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    user: &User,
    maybe_internal_activity_id: Option<Uuid>,
) -> Result<OutgoingActivity, DatabaseError> {
    let activity = build_update_person(&instance.url(), user, maybe_internal_activity_id)
        .map_err(|_| DatabaseTypeError)?;
    let recipients = get_update_person_recipients(db_client, &user.id).await?;
    Ok(OutgoingActivity::new(instance, user, activity, recipients))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitra_models::profiles::types::DbActorProfile;
    use mitra_utils::crypto_rsa::{generate_weak_rsa_key, serialize_private_key};

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_update_person() {
        let private_key = generate_weak_rsa_key().unwrap();
        let private_key_pem = serialize_private_key(&private_key).unwrap();
        let user = User {
            private_key: private_key_pem,
            profile: DbActorProfile {
                username: "testuser".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let internal_id = generate_ulid();
        let activity = build_update_person(INSTANCE_URL, &user, Some(internal_id)).unwrap();
        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, internal_id),
        );
        assert_eq!(
            activity.object.id,
            format!("{}/users/testuser", INSTANCE_URL),
        );
        assert_eq!(
            activity.to,
            vec![
                AP_PUBLIC.to_string(),
                format!("{}/users/testuser/followers", INSTANCE_URL),
            ]
        );
    }
}
