use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::{Post, Visibility},
    profiles::types::DbActorProfile,
    users::types::User,
};

use crate::activitypub::{
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id, profile_actor_id},
    types::{build_default_context, Context},
    vocabulary::UNDO,
};

use super::like::{
    get_like_audience,
    get_like_recipients,
};

#[derive(Serialize)]
struct UndoLike {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_undo_like(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    reaction_id: &Uuid,
    post_author_id: &str,
    post_visibility: &Visibility,
) -> UndoLike {
    let object_id = local_object_id(instance_url, reaction_id);
    let activity_id = format!("{}/undo", object_id);
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    let (primary_audience, secondary_audience) =
        get_like_audience(post_author_id, post_visibility);
    UndoLike {
        context: build_default_context(),
        activity_type: UNDO.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object_id,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn prepare_undo_like(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &User,
    post: &Post,
    reaction_id: &Uuid,
) -> Result<OutgoingActivity, DatabaseError> {
    let recipients = get_like_recipients(
        db_client,
        &instance.url(),
        post,
    ).await?;
    let post_author_id = profile_actor_id(&instance.url(), &post.author);
    let activity = build_undo_like(
        &instance.url(),
        &sender.profile,
        reaction_id,
        &post_author_id,
        &post.visibility,
    );
    Ok(OutgoingActivity::new(
        instance,
        sender,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use mitra_utils::id::generate_ulid;
    use crate::activitypub::constants::AP_PUBLIC;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_undo_like() {
        let author = DbActorProfile::default();
        let post_author_id = "https://example.com/users/test";
        let reaction_id = generate_ulid();
        let activity = build_undo_like(
            INSTANCE_URL,
            &author,
            &reaction_id,
            post_author_id,
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
        assert_eq!(activity.to, vec![post_author_id, AP_PUBLIC]);
    }
}
