use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio_postgres::GenericClient;

use crate::activitypub::{
    actors::types::Actor,
    constants::{AP_CONTEXT, AP_PUBLIC},
    deliverer::OutgoingActivity,
    identifiers::{local_actor_followers, local_actor_id, local_object_id},
    vocabulary::ANNOUNCE,
};
use crate::config::Instance;
use crate::database::DatabaseError;
use crate::models::posts::types::Post;
use crate::models::relationships::queries::get_followers;
use crate::models::users::types::User;

#[derive(Serialize)]
struct Announce {
    #[serde(rename = "@context")]
    context: String,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,
    published: DateTime<Utc>,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_announce(
    instance_url: &str,
    sender_username: &str,
    repost: &Post,
) -> Announce {
    let actor_id = local_actor_id(instance_url, sender_username);
    let post = repost.repost_of.as_ref().unwrap();
    let object_id = post.object_id(instance_url);
    let activity_id = local_object_id(instance_url, &repost.id);
    let recipient_id = post.author.actor_id(instance_url);
    let followers = local_actor_followers(instance_url, sender_username);
    Announce {
        context: AP_CONTEXT.to_string(),
        activity_type: ANNOUNCE.to_string(),
        actor: actor_id,
        id: activity_id,
        object: object_id,
        published: repost.created_at,
        to: vec![AP_PUBLIC.to_string(), recipient_id],
        cc: vec![followers],
    }
}

pub async fn get_announce_recipients(
    db_client: &impl GenericClient,
    instance_url: &str,
    current_user: &User,
    post: &Post,
) -> Result<(Vec<Actor>, String), DatabaseError> {
    let followers = get_followers(db_client, &current_user.id).await?;
    let mut recipients: Vec<Actor> = Vec::new();
    for profile in followers {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    };
    let primary_recipient = post.author.actor_id(instance_url);
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        recipients.push(remote_actor.clone());
    };
    Ok((recipients, primary_recipient))
}

pub async fn prepare_announce(
    db_client: &impl GenericClient,
    instance: &Instance,
    sender: &User,
    repost: &Post,
) -> Result<OutgoingActivity, DatabaseError> {
    let post = repost.repost_of.as_ref().unwrap();
    let (recipients, _) = get_announce_recipients(
        db_client,
        &instance.url(),
        sender,
        post,
    ).await?;
    let activity = build_announce(
        &instance.url(),
        &sender.profile.username,
        repost,
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
    use crate::activitypub::actors::types::Actor;
    use crate::models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_announce() {
        let post_author_id = "https://test.net/user/test";
        let post_author = DbActorProfile {
            actor_json: Some(Actor {
                id: post_author_id.to_string(),
                ..Default::default()
            }),
            actor_id: Some(post_author_id.to_string()),
            ..Default::default()
        };
        let post_id = "https://test.net/obj/123";
        let post = Post {
            author: post_author.clone(),
            object_id: Some(post_id.to_string()),
            ..Default::default()
        };
        let repost_author = DbActorProfile {
            username: "announcer".to_string(),
            ..Default::default()
        };
        let repost = Post {
            author: repost_author.clone(),
            repost_of_id: Some(post.id),
            repost_of: Some(Box::new(post)),
            ..Default::default()
        };
        let activity = build_announce(
            INSTANCE_URL,
            &repost_author.username,
            &repost,
        );
        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, repost.id),
        );
        assert_eq!(
            activity.actor,
            format!("{}/users/announcer", INSTANCE_URL),
        );
        assert_eq!(activity.object, post_id);
        assert_eq!(activity.to, vec![AP_PUBLIC, post_author_id]);
    }
}
