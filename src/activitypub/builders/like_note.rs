use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity},
    actor::Actor,
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    identifiers::local_object_id,
    vocabulary::LIKE,
};
use crate::config::Instance;
use crate::errors::DatabaseError;
use crate::models::posts::types::Post;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;

fn build_like_note(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    note_id: &str,
    reaction_id: &Uuid,
    recipient_id: &str,
) -> Activity {
    let activity_id = local_object_id(instance_url, reaction_id);
    let activity = create_activity(
        instance_url,
        &actor_profile.username,
        LIKE,
        activity_id,
        note_id,
        vec![AP_PUBLIC.to_string(), recipient_id.to_string()],
        vec![],
    );
    activity
}

pub async fn get_like_note_recipients(
    _db_client: &impl GenericClient,
    instance_url: &str,
    post: &Post,
) -> Result<(Vec<Actor>, String), DatabaseError> {
    let mut recipients: Vec<Actor> = Vec::new();
    let primary_recipient = post.author.actor_id(instance_url);
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        recipients.push(remote_actor.clone());
    };
    Ok((recipients, primary_recipient))
}

pub async fn prepare_like_note(
    db_client: &impl GenericClient,
    instance: Instance,
    user: &User,
    post: &Post,
    reaction_id: &Uuid,
) -> Result<OutgoingActivity<Activity>, DatabaseError> {
    let (recipients, primary_recipient) = get_like_note_recipients(
        db_client,
        &instance.url(),
        post,
    ).await?;
    let note_id = post.get_object_id(&instance.url());
    let activity = build_like_note(
        &instance.url(),
        &user.profile,
        &note_id,
        reaction_id,
        &primary_recipient,
    );
    Ok(OutgoingActivity {
        instance,
        sender: user.clone(),
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
        );
        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, reaction_id),
        );
        assert_eq!(activity.object, json!(note_id));
        assert_eq!(activity.to.unwrap(), json!([AP_PUBLIC, note_author_id]));
    }
}
