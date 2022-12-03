use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity},
    actors::types::Actor,
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    identifiers::local_object_id,
    vocabulary::LIKE,
};
use crate::config::Instance;
use crate::database::DatabaseError;
use crate::models::posts::types::{Post, Visibility};
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;

pub fn get_like_note_audience(
    note_author_id: &str,
    note_visibility: &Visibility,
) -> (Vec<String>, Vec<String>) {
    let mut primary_audience = vec![note_author_id.to_string()];
    if matches!(note_visibility, Visibility::Public) {
        primary_audience.push(AP_PUBLIC.to_string());
    };
    let secondary_audience = vec![];
    (primary_audience, secondary_audience)
}

fn build_like_note(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    note_id: &str,
    reaction_id: &Uuid,
    note_author_id: &str,
    note_visibility: &Visibility,
) -> Activity {
    let activity_id = local_object_id(instance_url, reaction_id);
    let (primary_audience, secondary_audience) =
        get_like_note_audience(note_author_id, note_visibility);
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        LIKE,
        activity_id,
        note_id,
        primary_audience,
        secondary_audience,
    );
    activity
}

pub async fn get_like_note_recipients(
    _db_client: &impl GenericClient,
    _instance_url: &str,
    post: &Post,
) -> Result<Vec<Actor>, DatabaseError> {
    let mut recipients: Vec<Actor> = Vec::new();
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        recipients.push(remote_actor.clone());
    };
    Ok(recipients)
}

pub async fn prepare_like_note(
    db_client: &impl GenericClient,
    instance: &Instance,
    sender: &User,
    post: &Post,
    reaction_id: &Uuid,
) -> Result<OutgoingActivity<Activity>, DatabaseError> {
    let recipients = get_like_note_recipients(
        db_client,
        &instance.url(),
        post,
    ).await?;
    let note_id = post.object_id(&instance.url());
    let note_author_id = post.author.actor_id(&instance.url());
    let activity = build_like_note(
        &instance.url(),
        &sender.profile,
        &note_id,
        reaction_id,
        &note_author_id,
        &post.visibility,
    );
    Ok(OutgoingActivity {
        instance: instance.clone(),
        sender: sender.clone(),
        activity,
        recipients,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::utils::id::new_uuid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_like_note() {
        let author = DbActorProfile::default();
        let note_id = "https://example.com/objects/123";
        let note_author_id = "https://example.com/users/test";
        let reaction_id = new_uuid();
        let activity = build_like_note(
            INSTANCE_URL,
            &author,
            note_id,
            &reaction_id,
            note_author_id,
            &Visibility::Public,
        );
        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, reaction_id),
        );
        assert_eq!(activity.object, json!(note_id));
        assert_eq!(activity.to.unwrap(), json!([note_author_id, AP_PUBLIC]));
    }
}
