use serde::Serialize;

use crate::activitypub::{
    deliverer::OutgoingActivity,
    identifiers::local_actor_id,
    types::{build_default_context, Context},
    vocabulary::{DELETE, NOTE, TOMBSTONE},
};
use crate::config::Instance;
use crate::database::{DatabaseClient, DatabaseError};
use crate::models::posts::helpers::add_related_posts;
use crate::models::posts::types::Post;
use crate::models::users::types::User;
use super::create_note::{
    build_note,
    get_note_recipients,
    Note,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Tombstone {
    id: String,

    #[serde(rename = "type")]
    object_type: String,

    former_type: String,
}

#[derive(Serialize)]
struct DeleteNote {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Tombstone,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_delete_note(
    instance_hostname: &str,
    instance_url: &str,
    post: &Post,
) -> DeleteNote {
    let object_id = post.object_id(instance_url);
    let activity_id = format!("{}/delete", object_id);
    let actor_id = local_actor_id(instance_url, &post.author.username);
    let Note { to, cc, .. } = build_note(
        instance_hostname,
        instance_url,
        post,
    );
    DeleteNote {
        context: build_default_context(),
        activity_type: DELETE.to_string(),
        id: activity_id,
        actor: actor_id,
        object: Tombstone {
            id: object_id,
            object_type: TOMBSTONE.to_string(),
            former_type: NOTE.to_string(),
        },
        to: to,
        cc: cc,
    }
}

pub async fn prepare_delete_note(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    author: &User,
    post: &Post,
) -> Result<OutgoingActivity, DatabaseError> {
    assert_eq!(author.id, post.author.id);
    let mut post = post.clone();
    add_related_posts(db_client, vec![&mut post]).await?;
    let activity = build_delete_note(
        &instance.hostname(),
        &instance.url(),
        &post,
    );
    let recipients = get_note_recipients(db_client, author, &post).await?;
    Ok(OutgoingActivity::new(
        instance,
        author,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use crate::activitypub::{
        constants::AP_PUBLIC,
        identifiers::local_actor_followers,
    };
    use crate::models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_HOSTNAME: &str = "example.com";
    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_delete_note() {
        let author = DbActorProfile {
            username: "author".to_string(),
            ..Default::default()
        };
        let post = Post { author, ..Default::default() };
        let activity = build_delete_note(
            INSTANCE_HOSTNAME,
            INSTANCE_URL,
            &post,
        );

        assert_eq!(
            activity.id,
            format!("{}/objects/{}/delete", INSTANCE_URL, post.id),
        );
        assert_eq!(
            activity.object.id,
            format!("{}/objects/{}", INSTANCE_URL, post.id),
        );
        assert_eq!(activity.object.object_type, "Tombstone");
        assert_eq!(activity.to, vec![AP_PUBLIC]);
        assert_eq!(
            activity.cc,
            vec![local_actor_followers(INSTANCE_URL, "author")],
        );
    }
}
