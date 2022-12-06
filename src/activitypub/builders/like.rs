use serde::Serialize;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::activitypub::{
    actors::types::Actor,
    constants::{AP_CONTEXT, AP_PUBLIC},
    deliverer::OutgoingActivity,
    identifiers::{local_actor_id, local_object_id},
    vocabulary::LIKE,
};
use crate::config::Instance;
use crate::database::DatabaseError;
use crate::models::posts::types::{Post, Visibility};
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::User;

#[derive(Serialize)]
struct Like {
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

pub fn get_like_audience(
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

fn build_like(
    instance_url: &str,
    actor_profile: &DbActorProfile,
    object_id: &str,
    reaction_id: &Uuid,
    post_author_id: &str,
    post_visibility: &Visibility,
) -> Like {
    let activity_id = local_object_id(instance_url, reaction_id);
    let actor_id = local_actor_id(instance_url, &actor_profile.username);
    let (primary_audience, secondary_audience) =
        get_like_audience(post_author_id, post_visibility);
    Like {
        context: AP_CONTEXT.to_string(),
        activity_type: LIKE.to_string(),
        id: activity_id,
        actor: actor_id,
        object: object_id.to_string(),
        to: primary_audience,
        cc: secondary_audience,
    }
}

pub async fn get_like_recipients(
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

pub async fn prepare_like(
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
    let object_id = post.object_id(&instance.url());
    let post_author_id = post.author.actor_id(&instance.url());
    let activity = build_like(
        &instance.url(),
        &sender.profile,
        &object_id,
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
    use crate::utils::id::new_uuid;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_like() {
        let author = DbActorProfile::default();
        let post_id = "https://example.com/objects/123";
        let post_author_id = "https://example.com/users/test";
        let reaction_id = new_uuid();
        let activity = build_like(
            INSTANCE_URL,
            &author,
            post_id,
            &reaction_id,
            post_author_id,
            &Visibility::Public,
        );
        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, reaction_id),
        );
        assert_eq!(activity.object, post_id);
        assert_eq!(activity.to, vec![post_author_id, AP_PUBLIC]);
        assert_eq!(activity.cc.is_empty(), true);
    }
}
