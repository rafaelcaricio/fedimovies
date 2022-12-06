use serde::Serialize;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    constants::AP_CONTEXT,
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id},
    vocabulary::UNDO,
};
use crate::config::Instance;
use crate::database::DatabaseError;
use crate::models::posts::types::{Post, Visibility};
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;
use super::like::{
    get_like_audience,
    get_like_recipients,
};

#[derive(Serialize)]
struct UndoLike {
    #[serde(rename = "@context")]
    context: String,

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
        context: AP_CONTEXT.to_string(),
        activity_type: UNDO.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object_id,
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn prepare_undo_like(
    db_client: &impl GenericClient,
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
    let post_author_id = post.author.actor_id(&instance.url());
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
    use crate::activitypub::constants::AP_PUBLIC;
    use crate::utils::id::new_uuid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_undo_like() {
        let author = DbActorProfile::default();
        let post_author_id = "https://example.com/users/test";
        let reaction_id = new_uuid();
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
