use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::{Activity, Object},
    vocabulary::{NOTE, PERSON},
};
use crate::config::Config;
use crate::errors::ValidationError;
use super::HandlerResult;
use super::update_note::handle_update_note;
use super::update_person::handle_update_person;

pub async fn handle_update(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    let object_type = activity.object["type"].as_str()
        .ok_or(ValidationError("unknown object type"))?;
    match object_type {
        NOTE => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            handle_update_note(db_client, object).await
        },
        PERSON => {
            handle_update_person(config, db_client, activity).await
        },
        _ => {
            log::warn!("unexpected object type {}", object_type);
            Ok(None)
        },
    }
}
