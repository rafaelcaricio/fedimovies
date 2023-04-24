use chrono::{DateTime, Utc};
use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::types::Post,
    profiles::types::DbActor,
    relationships::queries::get_followers,
    users::types::User,
};

use crate::activitypub::{
    constants::AP_PUBLIC,
    deliverer::OutgoingActivity,
    identifiers::{
        local_actor_followers, local_actor_id, local_object_id, post_object_id, profile_actor_id,
    },
    types::{build_default_context, Context},
    vocabulary::ANNOUNCE,
};

#[derive(Serialize)]
pub struct Announce {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,
    published: DateTime<Utc>,

    to: Vec<String>,
    cc: Vec<String>,
}

pub fn build_announce(instance_url: &str, repost: &Post) -> Announce {
    let actor_id = local_actor_id(instance_url, &repost.author.username);
    let post = repost
        .repost_of
        .as_ref()
        .expect("repost_of field should be populated");
    let object_id = post_object_id(instance_url, post);
    let activity_id = local_object_id(instance_url, &repost.id);
    let recipient_id = profile_actor_id(instance_url, &post.author);
    let followers = local_actor_followers(instance_url, &repost.author.username);
    Announce {
        context: build_default_context(),
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
    db_client: &impl DatabaseClient,
    instance_url: &str,
    current_user: &User,
    post: &Post,
) -> Result<(Vec<DbActor>, String), DatabaseError> {
    let followers = get_followers(db_client, &current_user.id).await?;
    let mut recipients = vec![];
    for profile in followers {
        if let Some(remote_actor) = profile.actor_json {
            recipients.push(remote_actor);
        };
    }
    let primary_recipient = profile_actor_id(instance_url, &post.author);
    if let Some(remote_actor) = post.author.actor_json.as_ref() {
        recipients.push(remote_actor.clone());
    };
    Ok((recipients, primary_recipient))
}

pub async fn prepare_announce(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    sender: &User,
    repost: &Post,
) -> Result<OutgoingActivity, DatabaseError> {
    assert_eq!(sender.id, repost.author.id);
    let post = repost
        .repost_of
        .as_ref()
        .expect("repost_of field should be populated");
    let (recipients, _) = get_announce_recipients(db_client, &instance.url(), sender, post).await?;
    let activity = build_announce(&instance.url(), repost);
    Ok(OutgoingActivity::new(
        instance, sender, activity, recipients,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitra_models::profiles::types::DbActorProfile;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_announce() {
        let post_author_id = "https://test.net/user/test";
        let post_author = DbActorProfile {
            actor_json: Some(DbActor {
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
            author: repost_author,
            repost_of_id: Some(post.id),
            repost_of: Some(Box::new(post)),
            ..Default::default()
        };
        let activity = build_announce(INSTANCE_URL, &repost);
        assert_eq!(
            activity.id,
            format!("{}/objects/{}", INSTANCE_URL, repost.id),
        );
        assert_eq!(activity.actor, format!("{}/users/announcer", INSTANCE_URL),);
        assert_eq!(activity.object, post_id);
        assert_eq!(activity.to, vec![AP_PUBLIC, post_author_id]);
    }
}
