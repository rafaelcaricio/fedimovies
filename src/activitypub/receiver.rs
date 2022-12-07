use actix_web::HttpRequest;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio_postgres::GenericClient;

use crate::config::Config;
use crate::database::DatabaseError;
use crate::errors::{
    ConversionError,
    HttpError,
    ValidationError,
};
use super::activity::Activity;
use super::authentication::{
    verify_signed_activity,
    verify_signed_request,
    AuthenticationError,
};
use super::fetcher::fetchers::FetchError;
use super::handlers::{
    accept_follow::handle_accept_follow,
    add::handle_add,
    announce::handle_announce,
    create_note::handle_create,
    delete::handle_delete,
    follow::handle_follow,
    like::handle_like,
    move_person::handle_move_person,
    reject_follow::handle_reject_follow,
    remove::handle_remove,
    undo::handle_undo,
    update::handle_update,
};
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

fn require_actor_signature(actor_id: &str, signer_id: &str)
    -> Result<(), AuthenticationError>
{
    if actor_id != signer_id {
        // Forwarded activity
        log::warn!(
            "request signer {} does not match actor {}",
            signer_id,
            actor_id,
        );
        return Err(AuthenticationError::UnexpectedSigner);
    };
    Ok(())
}

pub async fn receive_activity(
    config: &Config,
    db_client: &mut impl GenericClient,
    request: &HttpRequest,
    activity_raw: &Value,
) -> Result<(), HandlerError> {
    let activity: Activity = serde_json::from_value(activity_raw.clone())
        .map_err(|_| ValidationError("invalid activity"))?;
    let activity_type = activity.activity_type.clone();
    let activity_actor = activity.actor.clone();

    let is_self_delete = if activity_type == DELETE {
        let object_id = find_object_id(&activity.object)?;
        activity.actor == object_id
    } else { false };
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
            if is_self_delete {
                // Ignore Delete(Person) activities without HTTP signatures
                return Ok(());
            };
            log::warn!("invalid HTTP signature: {}", error);
            return Err(error.into());
        },
    };

    // Verify embedded signature
    match verify_signed_activity(config, db_client, activity_raw).await {
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
        log::warn!("ignoring activity from blocked instance: {}", activity_raw);
        return Ok(());
    };

    let signer_id = signer.actor_id(&config.instance_url());

    let maybe_object_type = match activity_type.as_str() {
        ACCEPT => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_accept_follow(config, db_client, activity).await?
        },
        REJECT => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_reject_follow(config, db_client, activity).await?
        },
        CREATE => {
            handle_create(config, db_client, activity, &signer_id).await?
        },
        ANNOUNCE => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_announce(config, db_client, activity).await?
        },
        DELETE => {
            if signer_id != activity.actor {
                // Ignore forwarded Delete() activities
                return Ok(());
            };
            handle_delete(config, db_client, activity).await?
        },
        EMOJI_REACT | LIKE => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_like(config, db_client, activity).await?
        },
        FOLLOW => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_follow(config, db_client, activity).await?
        },
        UNDO => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_undo(config, db_client, activity).await?
        },
        UPDATE => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_update(config, db_client, activity).await?
        },
        MOVE => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_move_person(config, db_client, activity).await?
        },
        ADD => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_add(config, db_client, activity).await?
        },
        REMOVE => {
            require_actor_signature(&activity.actor, &signer_id)?;
            handle_remove(config, db_client, activity).await?
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
