use actix_web::HttpRequest;
use chrono::{Duration, Utc};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    de::DeserializeOwned,
    de::Error as DeserializerError,
};
use serde_json::Value;
use tokio_postgres::GenericClient;

use crate::config::Config;
use crate::database::{DatabaseError, DatabaseTypeError};
use crate::errors::{
    ConversionError,
    HttpError,
    ValidationError,
};
use crate::models::{
    background_jobs::queries::{
        enqueue_job,
        get_job_batch,
        delete_job_from_queue,
    },
    background_jobs::types::JobType,
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

async fn handle_activity(
    config: &Config,
    db_client: &mut impl GenericClient,
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
    db_client: &mut impl GenericClient,
    request: &HttpRequest,
    activity: &Value,
) -> Result<(), HandlerError> {
    let activity_type = activity["type"].as_str()
        .ok_or(ValidationError("type property is missing"))?;
    let activity_actor = activity["actor"].as_str()
        .ok_or(ValidationError("actor property is missing"))?;

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
                AuthenticationError::DatabaseError(_)
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

    let signer_id = signer.actor_id(&config.instance_url());
    let is_authenticated = activity_actor == signer_id;
    if !is_authenticated {
        match activity_type {
            CREATE => (), // Accept forwarded Create() activities
            DELETE => {
                // Ignore forwarded Delete(Person) and Delete(Note) activities
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

    if let ANNOUNCE | CREATE | UPDATE = activity_type {
        // Add activity to job queue and release lock
        IncomingActivity::new(activity, is_authenticated)
            .enqueue(db_client, 0).await?;
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

#[derive(Deserialize, Serialize)]
struct IncomingActivity {
    activity: Value,
    is_authenticated: bool,
    failure_count: i32,
}

impl IncomingActivity {
    fn new(activity: &Value, is_authenticated: bool) -> Self {
        Self {
            activity: activity.clone(),
            is_authenticated,
            failure_count: 0,
        }
    }

    async fn enqueue(
        self,
        db_client: &impl GenericClient,
        delay: i64,
    ) -> Result<(), DatabaseError> {
        let job_data = serde_json::to_value(self)
            .expect("activity should be serializable");
        let scheduled_for = Utc::now() + Duration::seconds(delay);
        enqueue_job(
            db_client,
            &JobType::IncomingActivity,
            &job_data,
            &scheduled_for,
        ).await
    }
}

pub async fn process_queued_activities(
    config: &Config,
    db_client: &mut impl GenericClient,
) -> Result<(), DatabaseError> {
    let batch_size = 10;
    let max_retries = 2;
    let retry_after = 60 * 10; // 10 minutes

    let batch = get_job_batch(
        db_client,
        &JobType::IncomingActivity,
        batch_size,
    ).await?;
    for job in batch {
        let mut incoming_activity: IncomingActivity =
            serde_json::from_value(job.job_data)
                .map_err(|_| DatabaseTypeError)?;
        let is_error = match handle_activity(
            config,
            db_client,
            &incoming_activity.activity,
            incoming_activity.is_authenticated,
        ).await {
            Ok(_) => false,
            Err(error) => {
                incoming_activity.failure_count += 1;
                log::warn!(
                    "failed to process activity ({}) (attempt #{}): {}",
                    error,
                    incoming_activity.failure_count,
                    incoming_activity.activity,
                );
                true
            },
        };
        if is_error && incoming_activity.failure_count <= max_retries {
            // Re-queue
            log::info!("activity re-queued");
            incoming_activity.enqueue(db_client, retry_after).await?;
        };
        delete_job_from_queue(db_client, &job.id).await?;
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
