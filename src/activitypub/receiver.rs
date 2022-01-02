use std::collections::HashMap;

use regex::Regex;
use serde_json::Value;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{ConversionError, DatabaseError, HttpError, ValidationError};
use crate::models::attachments::queries::create_attachment;
use crate::models::posts::mentions::mention_to_address;
use crate::models::posts::queries::{
    create_post,
    get_post_by_id,
    get_post_by_object_id,
    delete_post,
};
use crate::models::posts::tags::normalize_tag;
use crate::models::posts::types::{Post, PostCreateData, Visibility};
use crate::models::profiles::queries::{
    get_profile_by_actor_id,
    get_profile_by_acct,
    update_profile,
};
use crate::models::profiles::types::ProfileUpdateData;
use crate::models::reactions::queries::{
    create_reaction,
    get_reaction_by_activity_id,
    delete_reaction,
};
use crate::models::relationships::queries::{
    follow_request_accepted,
    follow_request_rejected,
    follow,
    unfollow,
};
use crate::models::users::queries::get_user_by_name;
use super::activity::{Object, Activity, create_activity_accept_follow};
use super::actor::Actor;
use super::deliverer::deliver_activity;
use super::fetcher::fetchers::{
    fetch_avatar_and_banner,
    fetch_attachment,
    fetch_object,
};
use super::fetcher::helpers::{
    get_or_import_profile_by_actor_id,
    import_profile_by_actor_address,
    ImportError,
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
    let internal_object_id: Uuid = url_caps.name("uuid")
        .ok_or(ValidationError("invalid object ID"))?
        .as_str().parse()
        .map_err(|_| ValidationError("invalid object ID"))?;
    Ok(internal_object_id)
}

fn parse_array(value: &Value) -> Result<Vec<String>, ConversionError> {
    let result = match value {
        Value::String(string) => vec![string.to_string()],
        Value::Array(array) => {
            let mut results = vec![];
            for value in array {
                match value {
                    Value::String(string) => results.push(string.to_string()),
                    Value::Object(object) => {
                        if let Some(string) = object["id"].as_str() {
                            results.push(string.to_string());
                        } else {
                            // id property is missing
                            return Err(ConversionError);
                        };
                    },
                    // Unexpected array item type
                    _ => return Err(ConversionError),
                };
            };
            results
        },
        // Unexpected value type
        _ => return Err(ConversionError),
    };
    Ok(result)
}

/// Parses object json value and returns its ID as string
fn get_object_id(object: Value) -> Result<String, ValidationError> {
    let object_id = match object.as_str() {
        Some(object_id) => object_id.to_owned(),
        None => {
            let object: Object = serde_json::from_value(object)
                .map_err(|_| ValidationError("invalid object"))?;
            object.id
        },
    };
    Ok(object_id)
}

pub async fn process_note(
    config: &Config,
    db_client: &mut impl GenericClient,
    object_id: String,
    object_received: Option<Object>,
) -> Result<Post, HttpError> {
    let instance = config.instance();
    let mut maybe_object_id_to_fetch = Some(object_id);
    let mut maybe_object = object_received;
    let mut objects = vec![];
    let mut redirects: HashMap<String, String> = HashMap::new();
    let mut posts = vec![];

    // Fetch ancestors by going through inReplyTo references
    // TODO: fetch replies too
    #[allow(clippy::while_let_loop)]
    loop {
        let object_id = match maybe_object_id_to_fetch {
            Some(object_id) => {
                if parse_object_id(&instance.url(), &object_id).is_ok() {
                    // Object is a local post
                    assert!(objects.len() > 0);
                    break;
                }
                match get_post_by_object_id(db_client, &object_id).await {
                    Ok(post) => {
                        // Object already fetched
                        if objects.len() == 0 {
                            // Return post corresponding to initial object ID
                            return Ok(post);
                        };
                        break;
                    },
                    Err(DatabaseError::NotFound(_)) => (),
                    Err(other_error) => return Err(other_error.into()),
                };
                object_id
            },
            None => {
                // No object to fetch
                break;
            },
        };
        let object = match maybe_object {
            Some(object) => object,
            None => {
                let object = fetch_object(&instance, &object_id).await
                    .map_err(|err| {
                        log::warn!("{}", err);
                        ValidationError("failed to fetch object")
                    })?;
                log::info!("fetched object {}", object.id);
                object
            },
        };
        if object.id != object_id {
            // ID of fetched object doesn't match requested ID
            // Add IDs to the map of redirects
            redirects.insert(object_id, object.id.clone());
            maybe_object_id_to_fetch = Some(object.id.clone());
            // Don't re-fetch object on the next iteration
            maybe_object = Some(object);
        } else {
            maybe_object_id_to_fetch = object.in_reply_to.clone();
            maybe_object = None;
            objects.push(object);
        };
    }
    let initial_object_id = objects[0].id.clone();

    // Objects are ordered according to their place in reply tree,
    // starting with the root
    objects.reverse();
    for object in objects {
        let attributed_to = object.attributed_to
            .ok_or(ValidationError("unattributed note"))?;
        let author_id = parse_array(&attributed_to)
            .map_err(|_| ValidationError("invalid attributedTo property"))?
            .get(0)
            .ok_or(ValidationError("invalid attributedTo property"))?
            .to_string();
        let author = get_or_import_profile_by_actor_id(
            db_client,
            &instance,
            &config.media_dir(),
            &author_id,
        ).await?;
        let content = object.content
            .ok_or(ValidationError("no content"))?;
        let mut attachments: Vec<Uuid> = Vec::new();
        if let Some(list) = object.attachment {
            let mut downloaded = vec![];
            let output_dir = config.media_dir();
            for attachment in list {
                let (file_name, media_type) = fetch_attachment(&attachment.url, &output_dir).await
                    .map_err(|_| ValidationError("failed to fetch attachment"))?;
                log::info!("downloaded attachment {}", attachment.url);
                downloaded.push((
                    file_name,
                    attachment.media_type.or(media_type),
                ));
            }
            for (file_name, media_type) in downloaded {
                let db_attachment = create_attachment(
                    db_client,
                    &author.id,
                    file_name,
                    media_type,
                ).await?;
                attachments.push(db_attachment.id);
            }
        }
        let mut mentions: Vec<Uuid> = Vec::new();
        let mut tags = vec![];
        if let Some(list) = object.tag {
            for tag in list {
                if tag.tag_type == HASHTAG {
                    // Ignore invalid tags
                    if let Ok(tag_name) = normalize_tag(&tag.name) {
                        tags.push(tag_name);
                    };
                } else if tag.tag_type == MENTION {
                    // Ignore invalid mentions
                    if let Ok(actor_address) = mention_to_address(
                        &instance.host(),
                        &tag.name,
                    ) {
                        let profile = match get_profile_by_acct(
                            db_client,
                            &actor_address.acct(),
                        ).await {
                            Ok(profile) => profile,
                            Err(DatabaseError::NotFound(_)) => {
                                match import_profile_by_actor_address(
                                    db_client,
                                    &config.instance(),
                                    &config.media_dir(),
                                    &actor_address,
                                ).await {
                                    Ok(profile) => profile,
                                    Err(ImportError::FetchError(error)) => {
                                        // Ignore mention if fetcher fails
                                        log::warn!("{}", error);
                                        continue;
                                    },
                                    Err(other_error) => {
                                        return Err(other_error.into());
                                    },
                                }
                            },
                            Err(other_error) => return Err(other_error.into()),
                        };
                        if !mentions.contains(&profile.id) {
                            mentions.push(profile.id);
                        };
                    };
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
                        let note_id = redirects.get(&object_id)
                            .unwrap_or(&object_id);
                        let post = get_post_by_object_id(db_client, note_id).await?;
                        Some(post.id)
                    },
                }
            },
            None => None,
        };
        let visibility = match object.to {
            Some(value) => {
                let recipients = parse_array(&value)
                    .map_err(|_| ValidationError("invalid 'to' property value"))?;
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
            tags: tags,
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
    signer_id: &str,
    activity_raw: &Value,
) -> Result<(), HttpError> {
    let activity: Activity = serde_json::from_value(activity_raw.clone())
        .map_err(|_| ValidationError("invalid activity"))?;
    if activity.actor != signer_id {
        log::warn!(
            "request signer {} does not match actor {}",
            signer_id,
            activity.actor,
        );
    };
    let activity_type = activity.activity_type;
    let maybe_object_type = activity.object.get("type")
        .and_then(|val| val.as_str())
        .unwrap_or("Unknown");
    let db_client = &mut **get_database_client(db_pool).await?;
    let object_type = match (activity_type.as_str(), maybe_object_type) {
        (ACCEPT, FOLLOW) => {
            let object_id = get_object_id(activity.object)?;
            let follow_request_id = parse_object_id(&config.instance_url(), &object_id)?;
            follow_request_accepted(db_client, &follow_request_id).await?;
            FOLLOW
        },
        (REJECT, FOLLOW) => {
            let object_id = get_object_id(activity.object)?;
            let follow_request_id = parse_object_id(&config.instance_url(), &object_id)?;
            follow_request_rejected(db_client, &follow_request_id).await?;
            FOLLOW
        },
        (CREATE, NOTE) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            process_note(config, db_client, object.id.clone(), Some(object)).await?;
            NOTE
        },
        (ANNOUNCE, _) => {
            let repost_object_id = activity.id;
            match get_post_by_object_id(db_client, &repost_object_id).await {
                Ok(_) => return Ok(()), // Ignore if repost already exists
                Err(DatabaseError::NotFound(_)) => (),
                Err(other_error) => return Err(other_error.into()),
            };
            let author = get_or_import_profile_by_actor_id(
                db_client,
                &config.instance(),
                &config.media_dir(),
                &activity.actor,
            ).await?;
            let object_id = get_object_id(activity.object)?;
            let post_id = match parse_object_id(&config.instance_url(), &object_id) {
                Ok(post_id) => post_id,
                Err(_) => {
                    // Try to get remote post
                    let post = process_note(config, db_client, object_id, None).await?;
                    post.id
                },
            };
            let repost_data = PostCreateData {
                repost_of_id: Some(post_id),
                object_id: Some(repost_object_id),
                ..Default::default()
            };
            create_post(db_client, &author.id, repost_data).await?;
            NOTE
        },
        (DELETE, _) => {
            let object_id = get_object_id(activity.object)?;
            if object_id == activity.actor {
                log::info!("received deletion request for {}", object_id);
                // Ignore Delete(Person)
                return Ok(());
            };
            let post = match get_post_by_object_id(db_client, &object_id).await {
                Ok(post) => post,
                // Ignore Delete(Note) if post is not found
                Err(DatabaseError::NotFound(_)) => return Ok(()),
                Err(other_error) => return Err(other_error.into()),
            };
            let deletion_queue = delete_post(db_client, &post.id).await?;
            let config = config.clone();
            actix_rt::spawn(async move {
                deletion_queue.process(&config).await;
            });
            NOTE
        },
        (LIKE, _) | (EMOJI_REACT, _) => {
            let author = get_or_import_profile_by_actor_id(
                db_client,
                &config.instance(),
                &config.media_dir(),
                &activity.actor,
            ).await?;
            let object_id = get_object_id(activity.object)?;
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
            match create_reaction(
                db_client,
                &author.id,
                &post_id,
                Some(&activity.id),
            ).await {
                Ok(_) => (),
                // Ignore activity if reaction is already saved
                Err(DatabaseError::AlreadyExists(_)) => return Ok(()),
                Err(other_error) => return Err(other_error.into()),
            };
            NOTE
        },
        (FOLLOW, _) => {
            let source_profile = get_or_import_profile_by_actor_id(
                db_client,
                &config.instance(),
                &config.media_dir(),
                &activity.actor,
            ).await?;
            let source_actor = source_profile.actor_json
                .ok_or(HttpError::InternalError)?;
            let target_actor_id = get_object_id(activity.object)?;
            let target_username = parse_actor_id(&config.instance_url(), &target_actor_id)?;
            let target_user = get_user_by_name(db_client, &target_username).await?;
            match follow(db_client, &source_profile.id, &target_user.profile.id).await {
                Ok(_) => (),
                // Proceed even if relationship already exists
                Err(DatabaseError::AlreadyExists(_)) => (),
                Err(other_error) => return Err(other_error.into()),
            };

            // Send activity
            let new_activity = create_activity_accept_follow(
                &config.instance_url(),
                &target_user.profile,
                &activity.id,
                &source_actor.id,
            );
            let recipients = vec![source_actor];
            deliver_activity(config, &target_user, new_activity, recipients);
            PERSON
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
            FOLLOW
        },
        (UNDO, _) => {
            let object_id = get_object_id(activity.object)?;
            match get_reaction_by_activity_id(db_client, &object_id).await {
                Ok(reaction) => {
                    // Undo(Like)
                    delete_reaction(
                        db_client,
                        &reaction.author_id,
                        &reaction.post_id,
                    ).await?;
                    LIKE
                },
                Err(DatabaseError::NotFound(_)) => {
                    // Undo(Announce)
                    let post = match get_post_by_object_id(db_client, &object_id).await {
                        Ok(post) => post,
                        // Ignore undo if neither reaction nor repost is found
                        Err(DatabaseError::NotFound(_)) => return Ok(()),
                        Err(other_error) => return Err(other_error.into()),
                    };
                    match post.repost_of_id {
                        // Ignore returned data because reposts don't have attached files
                        Some(_) => delete_post(db_client, &post.id).await?,
                        // Can't undo regular post
                        None => return Err(HttpError::ValidationError("object is not a repost".into())),
                    };
                    ANNOUNCE
                },
                Err(other_error) => return Err(other_error.into()),
            }
        },
        (UPDATE, PERSON) => {
            let actor_value = activity.object.clone();
            let actor: Actor = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid actor data"))?;
            let profile = get_profile_by_actor_id(db_client, &actor.id).await?;
            let (avatar, banner) = fetch_avatar_and_banner(&actor, &config.media_dir()).await
                .map_err(|_| ValidationError("failed to fetch image"))?;
            let extra_fields = actor.extra_fields();
            let actor_old = profile.actor_json.unwrap();
            if actor_old.id != actor.id {
                log::warn!(
                    "actor ID changed from {} to {}",
                    actor_old.id,
                    actor.id,
                );
            };
            if actor_old.public_key.public_key_pem != actor.public_key.public_key_pem {
                log::warn!(
                    "actor public key changed from {} to {}",
                    actor_old.public_key.public_key_pem,
                    actor.public_key.public_key_pem,
                );
            };
            let mut profile_data = ProfileUpdateData {
                display_name: actor.name,
                bio: actor.summary.clone(),
                bio_source: actor.summary,
                avatar,
                banner,
                extra_fields,
                actor_json: Some(actor_value),
            };
            profile_data.clean()?;
            update_profile(db_client, &profile.id, profile_data).await?;
            PERSON
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
    use crate::utils::id::new_uuid;
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
        let expected_uuid = new_uuid();
        let object_id = format!(
            "https://example.org/objects/{}",
            expected_uuid,
        );
        let internal_object_id = parse_object_id(INSTANCE_URL, &object_id).unwrap();
        assert_eq!(internal_object_id, expected_uuid);
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

    #[test]
    fn test_parse_array_with_array_of_objects() {
        let value = json!([{"id": "test1"}, {"id": "test2"}]);
        assert_eq!(
            parse_array(&value).unwrap(),
            vec!["test1".to_string(), "test2".to_string()],
        );
    }

    #[test]
    fn test_get_object_id_from_string() {
        let value = json!("test_id");
        assert_eq!(get_object_id(value).unwrap(), "test_id");
    }

    #[test]
    fn test_get_object_id_from_object() {
        let value = json!({"id": "test_id", "type": "Note"});
        assert_eq!(get_object_id(value).unwrap(), "test_id");
    }
}
