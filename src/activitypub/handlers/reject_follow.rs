use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    receiver::{get_object_id, parse_object_id},
    vocabulary::FOLLOW,
};
use crate::config::Config;
use crate::errors::ValidationError;
use crate::models::profiles::queries::get_profile_by_actor_id;
use crate::models::relationships::queries::{
    follow_request_rejected,
    get_follow_request_by_id,
};
use super::HandlerResult;

pub async fn handle_reject_follow(
    config: &Config,
    db_client: &impl GenericClient,
    activity: Activity,
) -> HandlerResult {
    let actor_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
    let object_id = get_object_id(&activity.object)?;
    let follow_request_id = parse_object_id(&config.instance_url(), &object_id)?;
    let follow_request = get_follow_request_by_id(db_client, &follow_request_id).await?;
    if follow_request.target_id != actor_profile.id {
        return Err(ValidationError("actor is not a target").into());
    };
    follow_request_rejected(db_client, &follow_request_id).await?;
    Ok(Some(FOLLOW))
}
