use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity},
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    identifiers::local_object_id,
    vocabulary::UNDO,
};
use crate::config::Instance;
use crate::errors::DatabaseError;
use crate::models::posts::types::Post;
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;
use super::like_note::get_like_note_recipients;

fn build_undo_like(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    reaction_id: &Uuid,
    recipient_id: &str,
) -> Activity {
    let object_id = local_object_id(instance_url, reaction_id);
    let activity_id = format!("{}/undo", object_id);
    create_activity(
        instance_url,
        &actor_profile.username,
        UNDO,
        activity_id,
        object_id,
        vec![AP_PUBLIC.to_string(), recipient_id.to_string()],
        vec![],
    )
}

pub async fn prepare_undo_like_note(
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
    let activity = build_undo_like(
        &instance.url(),
        &user.profile,
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
    fn test_build_undo_like() {
        let author = DbActorProfile::default();
        let note_author_id = "https://example.com/users/test";
        let reaction_id = new_uuid();
        let activity = build_undo_like(
            INSTANCE_URL,
            &author,
            &reaction_id,
            note_author_id,
        );
        assert_eq!(
            activity.id,
            format!("{}/objects/{}/undo", INSTANCE_URL, reaction_id),
        );
        assert_eq!(
            activity.object,
            format!("{}/objects/{}", INSTANCE_URL, reaction_id),
        );
        assert_eq!(activity.to.unwrap(), json!([AP_PUBLIC, note_author_id]));
    }
}
