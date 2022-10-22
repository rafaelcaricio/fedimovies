use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    activity::{create_activity, Activity},
    deliverer::OutgoingActivity,
    identifiers::local_object_id,
    vocabulary::UNDO,
};
use crate::config::Instance;
use crate::errors::DatabaseError;
use crate::models::posts::types::{Post, Visibility};
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;
use super::like_note::{
    get_like_note_audience,
    get_like_note_recipients,
};

fn build_undo_like(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    reaction_id: &Uuid,
    note_author_id: &str,
    note_visibility: &Visibility,
) -> Activity {
    let object_id = local_object_id(instance_url, reaction_id);
    let activity_id = format!("{}/undo", object_id);
    let (primary_audience, secondary_audience) =
        get_like_note_audience(note_author_id, note_visibility);
    create_activity(
        instance_url,
        &actor_profile.username,
        UNDO,
        activity_id,
        object_id,
        primary_audience,
        secondary_audience,
    )
}

pub async fn prepare_undo_like_note(
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
    let note_author_id = post.author.actor_id(&instance.url());
    let activity = build_undo_like(
        &instance.url(),
        &sender.profile,
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
    use crate::activitypub::constants::AP_PUBLIC;
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
            &Visibility::Public,
        );
        assert_eq!(
            activity.id,
            format!("{}/objects/{}/undo", INSTANCE_URL, reaction_id),
        );
        assert_eq!(
            activity.object,
            format!("{}/objects/{}", INSTANCE_URL, reaction_id),
        );
        assert_eq!(activity.to.unwrap(), json!([note_author_id, AP_PUBLIC]));
    }
}
