use actix_web::HttpRequest;
use regex::Regex;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::config::Config;
use crate::errors::{ConversionError, DatabaseError, HttpError, ValidationError};
use crate::http_signatures::verify::verify_http_signature;
use crate::models::posts::queries::{
    create_post,
    get_post_by_object_id,
    delete_post,
};
use crate::models::posts::types::PostCreateData;
use crate::models::profiles::queries::{
    get_profile_by_actor_id,
    get_profile_by_acct,
};
use crate::models::reactions::queries::{
    create_reaction,
    get_reaction_by_activity_id,
    delete_reaction,
};
use crate::models::relationships::queries::{
    follow_request_accepted,
    follow_request_rejected,
    follow,
    get_follow_request_by_id,
    unfollow,
};
use crate::models::users::queries::get_user_by_name;
use super::activity::{
    Activity,
    Object,
    create_activity_accept_follow,
};
use super::deliverer::deliver_activity;
use super::fetcher::helpers::{
    get_or_import_profile_by_actor_id,
    import_post,
};
use super::inbox::update_note::handle_update_note;
use super::inbox::update_person::handle_update_person;
use super::vocabulary::*;

pub fn parse_actor_id(
    instance_url: &str,
    actor_id: &str,
) -> Result<String, ValidationError> {
    let url_regexp_str = format!(
        "^{}/users/(?P<username>[0-9a-z_]+)$",
        instance_url.replace('.', r"\."),
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

pub fn parse_object_id(
    instance_url: &str,
    object_id: &str,
) -> Result<Uuid, ValidationError> {
    let url_regexp_str = format!(
        "^{}/objects/(?P<uuid>[0-9a-f-]+)$",
        instance_url.replace('.', r"\."),
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

/// Transforms arbitrary property value into array of strings
pub fn parse_array(value: &Value) -> Result<Vec<String>, ConversionError> {
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

/// Transforms arbitrary property value into array of structs
pub fn parse_property_value<T: DeserializeOwned>(value: &Value) -> Result<Vec<T>, ConversionError> {
    let objects = match value {
        Value::Array(array) => array.to_vec(),
        Value::Object(_) => vec![value.clone()],
        // Unexpected value type
        _ => return Err(ConversionError),
    };
    let mut items = vec![];
    for object in objects {
        let item: T = serde_json::from_value(object)
            .map_err(|_| ConversionError)?;
        items.push(item);
    };
    Ok(items)
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

fn require_actor_signature(actor_id: &str, signer_id: &str)
    -> Result<(), HttpError>
{
    if actor_id != signer_id {
        // Forwarded activity
        log::warn!(
            "request signer {} does not match actor {}",
            signer_id,
            actor_id,
        );
        return Err(HttpError::AuthError("actor and request signer do not match"));
    };
    Ok(())
}

pub async fn receive_activity(
    config: &Config,
    db_client: &mut impl GenericClient,
    request: &HttpRequest,
    activity_raw: &Value,
) -> Result<(), HttpError> {
    let activity: Activity = serde_json::from_value(activity_raw.clone())
        .map_err(|_| ValidationError("invalid activity"))?;
    let activity_type = activity.activity_type.clone();
    let activity_actor = activity.actor.clone();
    let maybe_object_type = activity.object.get("type")
        .and_then(|val| val.as_str())
        .unwrap_or("Unknown");

    let signer = match verify_http_signature(config, db_client, request).await {
        Ok(signer) => signer,
        Err(error) => {
            let object_id = get_object_id(activity.object)?;
            if activity_type == DELETE && activity.actor == object_id {
                // Ignore Delete(Person) activities without HTTP signatures
                return Ok(());
            };
            log::warn!("invalid signature: {}", error);
            return Err(HttpError::AuthError("invalid signature"));
        },
    };
    let signer_id = signer.actor_id(&config.instance_url());
    log::debug!("activity signed by {}", signer_id);
    if config.blocked_instances.iter().any(|instance| signer.acct.contains(instance)) {
        return Err(HttpError::ValidationError("instance is blocked".into()));
    };

    let object_type = match (activity_type.as_str(), maybe_object_type) {
        (ACCEPT, FOLLOW) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            let actor_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
            let object_id = get_object_id(activity.object)?;
            let follow_request_id = parse_object_id(&config.instance_url(), &object_id)?;
            let follow_request = get_follow_request_by_id(db_client, &follow_request_id).await?;
            if follow_request.target_id != actor_profile.id {
                return Err(HttpError::ValidationError("actor is not a target".into()));
            };
            follow_request_accepted(db_client, &follow_request_id).await?;
            FOLLOW
        },
        (REJECT, FOLLOW) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            let actor_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
            let object_id = get_object_id(activity.object)?;
            let follow_request_id = parse_object_id(&config.instance_url(), &object_id)?;
            let follow_request = get_follow_request_by_id(db_client, &follow_request_id).await?;
            if follow_request.target_id != actor_profile.id {
                return Err(HttpError::ValidationError("actor is not a target".into()));
            };
            follow_request_rejected(db_client, &follow_request_id).await?;
            FOLLOW
        },
        (CREATE, NOTE | QUESTION | PAGE) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            let object_id = object.id.clone();
            let object_received = if activity.actor == signer_id {
                Some(object)
            } else {
                // Fetch forwarded note, don't trust the sender
                None
            };
            import_post(config, db_client, object_id, object_received).await?;
            NOTE
        },
        (ANNOUNCE, _) => {
            require_actor_signature(&activity.actor, &signer_id)?;
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
                    let post = import_post(config, db_client, object_id, None).await?;
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
            if signer_id != activity.actor {
                // Ignore forwarded Delete() activities
                return Ok(());
            };
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
            let actor_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
            if post.author.id != actor_profile.id {
                return Err(HttpError::ValidationError("actor is not an author".into()));
            };
            let deletion_queue = delete_post(db_client, &post.id).await?;
            let config = config.clone();
            actix_rt::spawn(async move {
                deletion_queue.process(&config).await;
            });
            NOTE
        },
        (EMOJI_REACT | LIKE, _) => {
            require_actor_signature(&activity.actor, &signer_id)?;
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
            require_actor_signature(&activity.actor, &signer_id)?;
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
            require_actor_signature(&activity.actor, &signer_id)?;
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            let source_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
            let target_actor_id = object.object
                .ok_or(ValidationError("invalid object"))?;
            let target_username = parse_actor_id(&config.instance_url(), &target_actor_id)?;
            let target_profile = get_profile_by_acct(db_client, &target_username).await?;
            match unfollow(db_client, &source_profile.id, &target_profile.id).await {
                Ok(_) => (),
                // Ignore Undo if relationship doesn't exist
                Err(DatabaseError::NotFound(_)) => return Ok(()),
                Err(other_error) => return Err(other_error.into()),
            };
            FOLLOW
        },
        (UNDO, _) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            let actor_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
            let object_id = get_object_id(activity.object)?;
            match get_reaction_by_activity_id(db_client, &object_id).await {
                Ok(reaction) => {
                    // Undo(Like)
                    if reaction.author_id != actor_profile.id {
                        return Err(HttpError::ValidationError("actor is not an author".into()));
                    };
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
                    if post.author.id != actor_profile.id {
                        return Err(HttpError::ValidationError("actor is not an author".into()));
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
        (UPDATE, NOTE) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            handle_update_note(db_client, &config.instance_url(), object).await?;
            NOTE
        },
        (UPDATE, PERSON) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_update_person(
                db_client,
                &config.media_dir(),
                activity,
            ).await?;
            PERSON
        },
        _ => {
            log::warn!("activity type is not supported: {}", activity_raw);
            return Ok(());
        },
    };
    log::info!(
        "processed {}({}) from {}",
        activity_type,
        object_type,
        activity_actor,
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
