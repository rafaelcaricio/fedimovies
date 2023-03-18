use actix_web::HttpRequest;
use serde::{
    Deserialize,
    Deserializer,
    de::DeserializeOwned,
    de::Error as DeserializerError,
};
use serde_json::Value;

use mitra_config::Config;

use crate::database::{DatabaseClient, DatabaseError};
use crate::errors::{
    ConversionError,
    HttpError,
    ValidationError,
};
use super::authentication::{
    verify_signed_activity,
    verify_signed_request,
    AuthenticationError,
};
use super::fetcher::fetchers::FetchError;
use super::handlers::{
    accept::handle_accept,
    add::handle_add,
    announce::handle_announce,
    create::handle_create,
    delete::handle_delete,
    follow::handle_follow,
    like::handle_like,
    r#move::handle_move,
    reject::handle_reject,
    remove::handle_remove,
    undo::handle_undo,
    update::handle_update,
};
use super::identifiers::profile_actor_id;
use super::queues::IncomingActivityJobData;
use super::vocabulary::*;

#[derive(thiserror::Error, Debug)]
pub enum HandlerError {
    #[error("local object")]
    LocalObject,

    #[error(transparent)]
    FetchError(#[from] FetchError),

    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),

    #[error(transparent)]
    AuthError(#[from] AuthenticationError),
}

impl From<HandlerError> for HttpError {
    fn from(error: HandlerError) -> Self {
        match error {
            HandlerError::LocalObject => HttpError::InternalError,
            HandlerError::FetchError(error) => {
                HttpError::ValidationError(error.to_string())
            },
            HandlerError::ValidationError(error) => error.into(),
            HandlerError::DatabaseError(error) => error.into(),
            HandlerError::AuthError(_) => {
                HttpError::AuthError("invalid signature")
            },
        }
    }
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
pub fn find_object_id(object: &Value) -> Result<String, ValidationError> {
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

pub fn deserialize_into_object_id<'de, D>(
    deserializer: D,
) -> Result<String, D::Error>
    where D: Deserializer<'de>
{
    let value = Value::deserialize(deserializer)?;
    let object_id = find_object_id(&value)
        .map_err(DeserializerError::custom)?;
    Ok(object_id)
}

pub async fn handle_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    activity: &Value,
    is_authenticated: bool,
) -> Result<(), HandlerError> {
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("type property is missing"))?
        .to_owned();
    let activity_actor = activity["actor"].as_str()
        .ok_or(ValidationError("actor property is missing"))?
        .to_owned();
    let activity = activity.clone();
    let maybe_object_type = match activity_type.as_str() {
        ACCEPT => {
            handle_accept(config, db_client, activity).await?
        },
        ADD => {
            handle_add(config, db_client, activity).await?
        },
        ANNOUNCE => {
            handle_announce(config, db_client, activity).await?
        },
        CREATE => {
            handle_create(config, db_client, activity, is_authenticated).await?
        },
        DELETE => {
            handle_delete(config, db_client, activity).await?
        },
        FOLLOW => {
            handle_follow(config, db_client, activity).await?
        },
        LIKE | EMOJI_REACT => {
            handle_like(config, db_client, activity).await?
        },
        MOVE => {
            handle_move(config, db_client, activity).await?
        },
        REJECT => {
            handle_reject(config, db_client, activity).await?
        },
        REMOVE => {
            handle_remove(config, db_client, activity).await?
        },
        UNDO => {
            handle_undo(config, db_client, activity).await?
        },
        UPDATE => {
            handle_update(config, db_client, activity).await?
        },
        _ => {
            log::warn!("activity type is not supported: {}", activity);
            None
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

pub async fn receive_activity(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    request: &HttpRequest,
    activity: &Value,
) -> Result<(), HandlerError> {
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("type property is missing"))?;
    let activity_actor = activity["actor"].as_str()
        .ok_or(ValidationError("actor property is missing"))?;

    let actor_hostname = url::Url::parse(activity_actor)
        .map_err(|_| ValidationError("invalid actor ID"))?
        .host_str()
        .ok_or(ValidationError("invalid actor ID"))?
        .to_string();
    if config.blocked_instances.iter()
        .any(|instance_hostname| &actor_hostname == instance_hostname)
    {
        log::warn!("ignoring activity from blocked instance: {}", activity);
        return Ok(());
    };

    let is_self_delete = if activity_type == DELETE {
        let object_id = find_object_id(&activity["object"])?;
        object_id == activity_actor
    } else { false };

    // HTTP signature is required
    let mut signer = match verify_signed_request(
        config,
        db_client,
        request,
        // Don't fetch signer if this is Delete(Person) activity
        is_self_delete,
    ).await {
        Ok(request_signer) => {
            log::debug!("request signed by {}", request_signer.acct);
            request_signer
        },
        Err(error) => {
            if is_self_delete && matches!(
                error,
                AuthenticationError::NoHttpSignature |
                AuthenticationError::DatabaseError(DatabaseError::NotFound(_))
            ) {
                // Ignore Delete(Person) activities without HTTP signatures
                // or if signer is not found in local database
                return Ok(());
            };
            log::warn!("invalid HTTP signature: {}", error);
            return Err(error.into());
        },
    };

    // JSON signature is optional
    match verify_signed_activity(
        config,
        db_client,
        activity,
        // Don't fetch actor if this is Delete(Person) activity
        is_self_delete,
    ).await {
        Ok(activity_signer) => {
            if activity_signer.acct != signer.acct {
                log::warn!(
                    "request signer {} is different from activity signer {}",
                    signer.acct,
                    activity_signer.acct,
                );
            } else {
                log::debug!("activity signed by {}", activity_signer.acct);
            };
            // Activity signature has higher priority
            signer = activity_signer;
        },
        Err(AuthenticationError::NoJsonSignature) => (), // ignore
        Err(other_error) => {
            log::warn!("invalid JSON signature: {}", other_error);
        },
    };

    if config.blocked_instances.iter()
        .any(|instance| signer.hostname.as_ref() == Some(instance))
    {
        log::warn!("ignoring activity from blocked instance: {}", activity);
        return Ok(());
    };

    let signer_id = profile_actor_id(&config.instance_url(), &signer);
    let is_authenticated = activity_actor == signer_id;
    if !is_authenticated {
        match activity_type {
            CREATE => (), // Accept forwarded Create() activities
            DELETE | LIKE => {
                // Ignore forwarded Delete and Like activities
                return Ok(());
            },
            _ => {
                // Reject other types
                log::warn!(
                    "request signer {} does not match actor {}",
                    signer_id,
                    activity_actor,
                );
                return Err(AuthenticationError::UnexpectedSigner.into());
            },
        };
    };

    if let ANNOUNCE | CREATE | DELETE | UNDO | UPDATE = activity_type {
        // Add activity to job queue and release lock
        IncomingActivityJobData::new(activity, is_authenticated)
            .into_job(db_client, 0).await?;
        log::debug!("activity added to the queue: {}", activity_type);
        return Ok(());
    };

    handle_activity(
        config,
        db_client,
        activity,
        is_authenticated,
    ).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

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
    fn test_parse_property_value_tag_list() {
        let value = json!({"type": "Mention"});
        let value_list: Vec<Value> = parse_property_value(&value).unwrap();
        assert_eq!(value_list, vec![value]);
    }

    #[test]
    fn test_find_object_id_from_string() {
        let value = json!("test_id");
        assert_eq!(find_object_id(&value).unwrap(), "test_id");
    }

    #[test]
    fn test_find_object_id_from_object() {
        let value = json!({"id": "test_id", "type": "Note"});
        assert_eq!(find_object_id(&value).unwrap(), "test_id");
    }
}
