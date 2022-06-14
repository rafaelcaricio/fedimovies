use actix_web::HttpRequest;
use regex::Regex;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::config::Config;
use crate::errors::{ConversionError, HttpError, ValidationError};
use crate::http_signatures::verify::verify_http_signature;
use super::activity::{Activity, Object};
use super::fetcher::helpers::import_post;
use super::handlers::{
    accept_follow::handle_accept_follow,
    announce::handle_announce,
    delete::handle_delete,
    follow::handle_follow,
    like::handle_like,
    reject_follow::handle_reject_follow,
    undo::handle_undo,
    undo_follow::handle_undo_follow,
    update_note::handle_update_note,
    update_person::handle_update_person,
};
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
pub fn get_object_id(object: &Value) -> Result<String, ValidationError> {
    let object_id = match object.as_str() {
        Some(object_id) => object_id.to_owned(),
        None => {
            let object_id = object["id"].as_str()
                .ok_or(ValidationError("missing object ID"))?
                .to_string();
            object_id
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

    let is_self_delete = if activity_type == DELETE {
        let object_id = get_object_id(&activity.object)?;
        activity.actor == object_id
    } else { false };
    // Don't fetch signer if this is Delete(Person) activity
    let signer = match verify_http_signature(config, db_client, request, is_self_delete).await {
        Ok(signer) => signer,
        Err(error) => {
            if is_self_delete {
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
        log::warn!("ignoring activity from blocked instance: {}", activity_raw);
        return Ok(());
    };

    let maybe_object_type = match (activity_type.as_str(), maybe_object_type) {
        (ACCEPT, FOLLOW) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_accept_follow(config, db_client, activity).await?
        },
        (REJECT, FOLLOW) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_reject_follow(config, db_client, activity).await?
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
            Some(NOTE)
        },
        (ANNOUNCE, _) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_announce(config, db_client, activity).await?
        },
        (DELETE, _) => {
            if signer_id != activity.actor {
                // Ignore forwarded Delete() activities
                return Ok(());
            };
            handle_delete(config, db_client, activity).await?
        },
        (EMOJI_REACT | LIKE, _) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_like(config, db_client, activity).await?
        },
        (FOLLOW, _) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_follow(config, db_client, activity).await?
        },
        (UNDO, FOLLOW) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_undo_follow(config, db_client, activity).await?
        },
        (UNDO, _) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_undo(db_client, activity).await?
        },
        (UPDATE, NOTE) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            handle_update_note(db_client, &config.instance_url(), object).await?
        },
        (UPDATE, PERSON) => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_update_person(db_client, &config.media_dir(), activity).await?
        },
        _ => {
            log::warn!("activity type is not supported: {}", activity_raw);
            return Ok(());
        },
    };
    if let Some(object_type) = maybe_object_type {
        log::info!(
            "processed {}({}) from {}",
            activity_type,
            object_type,
            activity_actor,
        );
    };
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
        assert_eq!(get_object_id(&value).unwrap(), "test_id");
    }

    #[test]
    fn test_get_object_id_from_object() {
        let value = json!({"id": "test_id", "type": "Note"});
        assert_eq!(get_object_id(&value).unwrap(), "test_id");
    }
}
