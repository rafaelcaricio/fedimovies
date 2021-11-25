use std::path::Path;

use regex::Regex;
use serde_json::Value;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::config::{Config, Instance};
use crate::database::{Pool, get_database_client};
use crate::errors::{DatabaseError, HttpError, ValidationError};
use crate::models::attachments::queries::create_attachment;
use crate::models::posts::types::{Post, PostCreateData, Visibility};
use crate::models::posts::queries::{
    create_post,
    get_post_by_id,
    get_post_by_object_id,
    delete_post,
};
use crate::models::profiles::queries::{
    get_profile_by_actor_id,
    get_profile_by_acct,
    create_profile,
    update_profile,
};
use crate::models::profiles::types::{DbActorProfile, ProfileUpdateData};
use crate::models::reactions::queries::create_reaction;
use crate::models::relationships::queries::{
    follow_request_accepted,
    follow_request_rejected,
    follow,
    unfollow,
};
use crate::models::users::queries::get_user_by_id;
use super::activity::{Object, Activity, create_activity_accept_follow};
use super::actor::Actor;
use super::deliverer::deliver_activity;
use super::fetcher::{
    fetch_avatar_and_banner,
    fetch_profile_by_actor_id,
    fetch_attachment,
    fetch_object,
};
use super::vocabulary::*;

fn parse_actor_id(
    instance_url: &str,
    actor_id: &str,
) -> Result<String, ValidationError> {
    let url_regexp_str = format!(
        "^{}/users/(?P<username>[0-9a-z_]+)$",
        instance_url.replace(".", r"\."),
    );
    let url_regexp = Regex::new(&url_regexp_str)
        .map_err(|_| ValidationError("error"))?;
    let url_caps = url_regexp.captures(actor_id)
        .ok_or(ValidationError("invalid actor ID"))?;
    let username = url_caps.name("username")
        .ok_or(ValidationError("invalid actor ID"))?
        .as_str()
        .to_owned();
    Ok(username)
}

fn parse_object_id(
    instance_url: &str,
    object_id: &str,
) -> Result<Uuid, ValidationError> {
    let url_regexp_str = format!(
        "^{}/objects/(?P<uuid>[0-9a-f-]+)$",
        instance_url.replace(".", r"\."),
    );
    let url_regexp = Regex::new(&url_regexp_str)
        .map_err(|_| ValidationError("error"))?;
    let url_caps = url_regexp.captures(object_id)
        .ok_or(ValidationError("invalid object ID"))?;
    let object_uuid: Uuid = url_caps.name("uuid")
        .ok_or(ValidationError("invalid object ID"))?
        .as_str().parse()
        .map_err(|_| ValidationError("invalid object ID"))?;
    Ok(object_uuid)
}

fn parse_array(value: &Value) -> Result<Vec<String>, ValidationError> {
    let result = match value {
        Value::String(string) => vec![string.to_string()],
        Value::Array(array) => {
            array.iter()
                .filter_map(|val| val.as_str().map(|s| s.to_string()))
                .collect()
        },
        _ => return Err(ValidationError("invalid attribute value")),
    };
    Ok(result)
}

async fn get_or_fetch_profile_by_actor_id(
    db_client: &impl GenericClient,
    instance: &Instance,
    actor_id: &str,
    media_dir: &Path,
) -> Result<DbActorProfile, HttpError> {
    let profile = match get_profile_by_actor_id(db_client, actor_id).await {
        Ok(profile) => profile,
        Err(DatabaseError::NotFound(_)) => {
            let profile_data = fetch_profile_by_actor_id(
                instance, actor_id, media_dir,
            )
                .await
                .map_err(|_| ValidationError("failed to fetch actor"))?;
            let profile = create_profile(db_client, &profile_data).await?;
            profile
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(profile)
}

pub async fn process_note(
    config: &Config,
    db_client: &mut impl GenericClient,
    object: Object,
) -> Result<Post, HttpError> {
    match get_post_by_object_id(db_client, &object.id).await {
        Ok(post) => return Ok(post), // post already exists
        Err(DatabaseError::NotFound(_)) => (), // continue processing
        Err(other_error) => return Err(other_error.into()),
    };

    let instance = config.instance();
    let initial_object_id = object.id.clone();
    let mut maybe_parent_object_id = object.in_reply_to.clone();
    let mut objects = vec![object];
    let mut posts = vec![];

    // Fetch ancestors by going through inReplyTo references
    // TODO: fetch replies too
    #[allow(clippy::while_let_loop)]
    loop {
        let object_id = match maybe_parent_object_id {
            Some(parent_object_id) => {
                if parse_object_id(&instance.url(), &parent_object_id).is_ok() {
                    // Parent object is a local post
                    break;
                }
                match get_post_by_object_id(db_client, &parent_object_id).await {
                    Ok(_) => {
                        // Parent object has been fetched already
                        break;
                    },
                    Err(DatabaseError::NotFound(_)) => (),
                    Err(other_error) => return Err(other_error.into()),
                };
                parent_object_id
            },
            None => {
                // Object does not have a parent
                break;
            },
        };
        let object = fetch_object(&instance, &object_id).await
            .map_err(|_| ValidationError("failed to fetch object"))?;
        maybe_parent_object_id = object.in_reply_to.clone();
        objects.push(object);
    }

    // Objects are ordered according to their place in reply tree,
    // starting with the root
    objects.reverse();
    for object in objects {
        let attributed_to = object.attributed_to
            .ok_or(ValidationError("unattributed note"))?;
        let author = get_or_fetch_profile_by_actor_id(
            db_client,
            &instance,
            &attributed_to,
            &config.media_dir(),
        ).await?;
        let content = object.content
            .ok_or(ValidationError("no content"))?;
        let mut attachments: Vec<Uuid> = Vec::new();
        if let Some(list) = object.attachment {
            let mut downloaded: Vec<(String, String)> = Vec::new();
            let output_dir = config.media_dir();
            for attachment in list {
                let file_name = fetch_attachment(&attachment.url, &output_dir).await
                    .map_err(|_| ValidationError("failed to fetch attachment"))?;
                log::info!("downloaded attachment {}", attachment.url);
                downloaded.push((file_name, attachment.media_type));
            }
            for (file_name, media_type) in downloaded {
                let db_attachment = create_attachment(
                    db_client,
                    &author.id,
                    Some(media_type),
                    file_name,
                ).await?;
                attachments.push(db_attachment.id);
            }
        }
        let mut mentions: Vec<Uuid> = Vec::new();
        if let Some(list) = object.tag {
            for tag in list {
                if tag.tag_type != MENTION {
                    continue;
                };
                if let Some(href) = tag.href {
                    let profile = get_or_fetch_profile_by_actor_id(
                        db_client,
                        &instance,
                        &href,
                        &config.media_dir(),
                    ).await?;
                    mentions.push(profile.id);
                };
            };
        };
        let in_reply_to_id = match object.in_reply_to {
            Some(object_id) => {
                match parse_object_id(&instance.url(), &object_id) {
                    Ok(post_id) => {
                        // Local post
                        let post = get_post_by_id(db_client, &post_id).await?;
                        Some(post.id)
                    },
                    Err(_) => {
                        let post = get_post_by_object_id(db_client, &object_id).await?;
                        Some(post.id)
                    },
                }
            },
            None => None,
        };
        let visibility = match object.to {
            Some(value) => {
                let recipients = parse_array(&value)?;
                if recipients.len() == 1 &&
                    parse_actor_id(&instance.url(), &recipients[0]).is_ok()
                {
                    // Single local recipient
                    Visibility::Direct
                } else {
                    Visibility::Public
                }
            },
            None => Visibility::Public,
        };
        let post_data = PostCreateData {
            content,
            in_reply_to_id,
            repost_of_id: None,
            visibility,
            attachments: attachments,
            mentions: mentions,
            object_id: Some(object.id),
            created_at: object.published,
        };
        let post = create_post(db_client, &author.id, post_data).await?;
        posts.push(post);
    }

    let initial_post = posts.into_iter()
        .find(|post| post.object_id.as_ref() == Some(&initial_object_id))
        .unwrap();
    Ok(initial_post)
}

pub async fn receive_activity(
    config: &Config,
    db_pool: &Pool,
    activity_raw: Value,
) -> Result<(), HttpError> {
    let activity: Activity = serde_json::from_value(activity_raw)
        .map_err(|_| ValidationError("invalid activity"))?;
    let activity_type = activity.activity_type;
    let object_type = activity.object.get("type")
        .and_then(|val| val.as_str())
        .unwrap_or("Unknown")
        .to_owned();
    let db_client = &mut **get_database_client(db_pool).await?;
    match (activity_type.as_str(), object_type.as_str()) {
        (ACCEPT, FOLLOW) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            let follow_request_id = parse_object_id(&config.instance_url(), &object.id)?;
            follow_request_accepted(db_client, &follow_request_id).await?;
        },
        (REJECT, FOLLOW) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            let follow_request_id = parse_object_id(&config.instance_url(), &object.id)?;
            follow_request_rejected(db_client, &follow_request_id).await?;
        },
        (CREATE, NOTE) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            process_note(config, db_client, object).await?;
        },
        (ANNOUNCE, _) => {
            let author = get_or_fetch_profile_by_actor_id(
                db_client,
                &config.instance(),
                &activity.actor,
                &config.media_dir(),
            ).await?;
            let object_id = match activity.object.as_str() {
                Some(object_id) => object_id.to_owned(),
                None => {
                    let object: Object = serde_json::from_value(activity.object)
                        .map_err(|_| ValidationError("invalid object"))?;
                    object.id
                },
            };
            let post_id = match parse_object_id(&config.instance_url(), &object_id) {
                Ok(post_id) => post_id,
                Err(_) => {
                    let post = get_post_by_object_id(db_client, &object_id).await?;
                    post.id
                },
            };
            let repost_data = PostCreateData {
                repost_of_id: Some(post_id),
                object_id: Some(object_id),
                ..Default::default()
            };
            create_post(db_client, &author.id, repost_data).await?;
        },
        (DELETE, _) => {
            let object_id = match activity.object.as_str() {
                Some(object_id) => object_id.to_owned(),
                None => {
                    let object: Object = serde_json::from_value(activity.object)
                        .map_err(|_| ValidationError("invalid object"))?;
                    object.id
                },
            };
            let post = get_post_by_object_id(db_client, &object_id).await?;
            let deletion_queue = delete_post(db_client, &post.id).await?;
            let config = config.clone();
            actix_rt::spawn(async move {
                deletion_queue.process(&config).await;
            });
        },
        (LIKE, _) => {
            let author = get_or_fetch_profile_by_actor_id(
                db_client,
                &config.instance(),
                &activity.actor,
                &config.media_dir(),
            ).await?;
            let object_id = match activity.object.as_str() {
                Some(object_id) => object_id.to_owned(),
                None => {
                    let object: Object = serde_json::from_value(activity.object)
                        .map_err(|_| ValidationError("invalid object"))?;
                    object.id
                },
            };
            let post_id = match parse_object_id(&config.instance_url(), &object_id) {
                Ok(post_id) => post_id,
                Err(_) => {
                    let post = match get_post_by_object_id(db_client, &object_id).await {
                        Ok(post) => post,
                        // Ignore like if post is not found locally
                        Err(DatabaseError::NotFound(_)) => return Ok(()),
                        Err(other_error) => return Err(other_error.into()),
                    };
                    post.id
                },
            };
            create_reaction(db_client, &author.id, &post_id).await?;
        },
        (FOLLOW, _) => {
            let source_profile = get_or_fetch_profile_by_actor_id(
                db_client,
                &config.instance(),
                &activity.actor,
                &config.media_dir(),
            ).await?;
            let source_actor = source_profile.remote_actor().ok().flatten()
                .ok_or(HttpError::InternalError)?;
            let target_actor_id = match activity.object.as_str() {
                Some(object_id) => object_id.to_owned(),
                None => {
                    let object: Object = serde_json::from_value(activity.object)
                        .map_err(|_| ValidationError("invalid object"))?;
                    object.id
                },
            };
            let target_username = parse_actor_id(&config.instance_url(), &target_actor_id)?;
            let target_profile = get_profile_by_acct(db_client, &target_username).await?;
            // Create and send 'Accept' activity
            let target_user = get_user_by_id(db_client, &target_profile.id).await?;
            let new_activity = create_activity_accept_follow(
                &config.instance_url(),
                &target_profile,
                &activity.id,
                &source_actor.id,
            );
            // Save relationship
            follow(db_client, &source_profile.id, &target_profile.id).await?;

            // Send activity
            let recipients = vec![source_actor];
            deliver_activity(config, &target_user, new_activity, recipients);
        },
        (UNDO, FOLLOW) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            let source_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
            let target_actor_id = object.object
                .ok_or(ValidationError("invalid object"))?;
            let target_username = parse_actor_id(&config.instance_url(), &target_actor_id)?;
            let target_profile = get_profile_by_acct(db_client, &target_username).await?;
            unfollow(db_client, &source_profile.id, &target_profile.id).await?;
        },
        (UPDATE, PERSON) => {
            let actor: Actor = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid actor data"))?;
            let profile = get_profile_by_actor_id(db_client, &actor.id).await?;
            let (avatar, banner) = fetch_avatar_and_banner(&actor, &config.media_dir()).await
                .map_err(|_| ValidationError("failed to fetch image"))?;
            let extra_fields = actor.extra_fields();
            let mut profile_data = ProfileUpdateData {
                display_name: Some(actor.name),
                bio: actor.summary.clone(),
                bio_source: actor.summary,
                avatar,
                banner,
                extra_fields,
            };
            profile_data.clean()?;
            update_profile(db_client, &profile.id, profile_data).await?;
        },
        _ => {
            return Err(HttpError::ValidationError("activity type is not supported".into()));
        },
    };
    log::info!(
        "processed {}({}) from {}",
        activity_type,
        object_type,
        activity.actor,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    const INSTANCE_URL: &str = "https://example.org";

    #[test]
    fn test_parse_actor_id() {
        let username = parse_actor_id(INSTANCE_URL, "https://example.org/users/test").unwrap();
        assert_eq!(username, "test".to_string());
    }

    #[test]
    fn test_parse_actor_id_wrong_path() {
        let error = parse_actor_id(INSTANCE_URL, "https://example.org/user/test").unwrap_err();
        assert_eq!(error.to_string(), "invalid actor ID");
    }

    #[test]
    fn test_parse_actor_id_invalid_username() {
        let error = parse_actor_id(INSTANCE_URL, "https://example.org/users/tes-t").unwrap_err();
        assert_eq!(error.to_string(), "invalid actor ID");
    }

    #[test]
    fn test_parse_actor_id_invalid_instance_url() {
        let error = parse_actor_id(INSTANCE_URL, "https://example.gov/users/test").unwrap_err();
        assert_eq!(error.to_string(), "invalid actor ID");
    }

    #[test]
    fn test_parse_object_id() {
        let expected_uuid = Uuid::new_v4();
        let object_id = format!(
            "https://example.org/objects/{}",
            expected_uuid,
        );
        let object_uuid = parse_object_id(INSTANCE_URL, &object_id).unwrap();
        assert_eq!(object_uuid, expected_uuid);
    }

    #[test]
    fn test_parse_object_id_invalid_uuid() {
        let object_id = "https://example.org/objects/1234";
        let error = parse_object_id(INSTANCE_URL, object_id).unwrap_err();
        assert_eq!(error.to_string(), "invalid object ID");
    }

    #[test]
    fn test_parse_array_with_string() {
        let value = json!("test");
        assert_eq!(
            parse_array(&value).unwrap(),
            vec!["test".to_string()],
        );
    }

    #[test]
    fn test_parse_array_with_array() {
        let value = json!(["test1", "test2"]);
        assert_eq!(
            parse_array(&value).unwrap(),
            vec!["test1".to_string(), "test2".to_string()],
        );
    }
}
