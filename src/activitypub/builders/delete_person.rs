use serde::Serialize;
use uuid::Uuid;

use fedimovies_config::Instance;
use fedimovies_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActor,
    relationships::queries::{get_followers, get_following},
    users::types::User,
};

use crate::activitypub::{
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    identifiers::local_actor_id,
    types::{build_default_context, Context},
    vocabulary::DELETE,
};

#[derive(Serialize)]
struct DeletePerson {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,

    to: Vec<String>,
}

fn build_delete_person(instance_url: &str, user: &User) -> DeletePerson {
    let actor_id = local_actor_id(instance_url, &user.profile.username);
    let activity_id = format!("{}/delete", actor_id);
    DeletePerson {
        context: build_default_context(),
        activity_type: DELETE.to_string(),
        id: activity_id,
        actor: actor_id.clone(),
        object: actor_id,
        to: vec![AP_PUBLIC.to_string()],
    }
}

async fn get_delete_person_recipients(
    db_client: &impl DatabaseClient,
    user_id: &Uuid,
) -> Result<Vec<DbActor>, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let following = get_following(db_client, user_id).await?;
    let mut recipients = vec![];
    for profile in followers.into_iter().chain(following.into_iter()) {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    }
    Ok(recipients)
}

pub async fn prepare_delete_person(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    user: &User,
) -> Result<OutgoingActivity, DatabaseError> {
    let activity = build_delete_person(&instance.url(), user);
    let recipients = get_delete_person_recipients(db_client, &user.id).await?;
    Ok(OutgoingActivity::new(instance, user, activity, recipients))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fedimovies_models::profiles::types::DbActorProfile;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_delete_person() {
        let user = User {
            profile: DbActorProfile {
                username: "testuser".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        let activity = build_delete_person(INSTANCE_URL, &user);
        assert_eq!(
            activity.id,
            format!("{}/users/testuser/delete", INSTANCE_URL),
        );
        assert_eq!(activity.actor, activity.object);
        assert_eq!(activity.object, format!("{}/users/testuser", INSTANCE_URL),);
        assert_eq!(activity.to, vec![AP_PUBLIC]);
    }
}
