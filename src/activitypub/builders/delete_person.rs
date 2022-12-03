use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity},
    actors::types::Actor,
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    vocabulary::DELETE,
};
use crate::config::Instance;
use crate::database::DatabaseError;
use crate::models::relationships::queries::{get_followers, get_following};
use crate::models::users::types::User;

fn build_delete_person(
    instance_url: &str,
    user: &User,
) -> Activity {
    let actor_id = user.profile.actor_id(instance_url);
    let activity_id = format!("{}/delete", actor_id);
    create_activity(
        instance_url,
        &user.profile.username,
        DELETE,
        activity_id,
        actor_id,
        vec![AP_PUBLIC.to_string()],
        vec![],
    )
}

async fn get_delete_person_recipients(
    db_client: &impl GenericClient,
    user_id: &Uuid,
) -> Result<Vec<Actor>, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let following = get_following(db_client, user_id).await?;
    let mut recipients = vec![];
    for profile in followers.into_iter().chain(following.into_iter()) {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    Ok(recipients)
}

pub async fn prepare_delete_person(
    db_client: &impl GenericClient,
    instance: &Instance,
    user: &User,
) -> Result<OutgoingActivity<Activity>, DatabaseError> {
    let activity = build_delete_person(&instance.url(), user);
    let recipients = get_delete_person_recipients(db_client, &user.id).await?;
    Ok(OutgoingActivity {
        instance: instance.clone(),
        sender: user.clone(),
        activity,
        recipients,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::models::profiles::types::DbActorProfile;
    use super::*;

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
        assert_eq!(
            activity.object,
            format!("{}/users/testuser", INSTANCE_URL),
        );
        assert_eq!(activity.to.unwrap(), json!([AP_PUBLIC]));
    }
}
