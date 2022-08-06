use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity},
    actors::types::{get_local_actor, Actor, ActorKeyError},
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    identifiers::{local_actor_followers, local_object_id},
    vocabulary::UPDATE,
};
use crate::config::Instance;
use crate::errors::{ConversionError, DatabaseError};
use crate::models::relationships::queries::get_followers;
use crate::models::users::types::User;
use crate::utils::id::new_uuid;

fn build_update_person(
    instance_url: &str,
    user: &User,
) -> Result<Activity, ActorKeyError> {
    let actor = get_local_actor(user, instance_url)?;
    // Update(Person) is idempotent so its ID can be random
    let activity_id = local_object_id(instance_url, &new_uuid());
    let activity = create_activity(
        instance_url,
        &user.profile.username,
        UPDATE,
        activity_id,
        actor,
        vec![
            AP_PUBLIC.to_string(),
            local_actor_followers(instance_url, &user.profile.username),
        ],
        vec![],
    );
    Ok(activity)
}

async fn get_update_person_recipients(
    db_client: &impl GenericClient,
    user_id: &Uuid,
) -> Result<Vec<Actor>, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let mut recipients: Vec<Actor> = Vec::new();
    for profile in followers {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    Ok(recipients)
}

pub async fn prepare_update_person(
    db_client: &impl GenericClient,
    instance: Instance,
    user: &User,
) -> Result<OutgoingActivity<Activity>, DatabaseError> {
    let activity = build_update_person(&instance.url(), user)
        .map_err(|_| ConversionError)?;
    let recipients = get_update_person_recipients(db_client, &user.id).await?;
    Ok(OutgoingActivity {
        instance,
        sender: user.clone(),
        activity,
        recipients,
    })
}
