use serde_json::Value;
use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    identifiers::parse_local_object_id,
    receiver::find_object_id,
    vocabulary::FOLLOW,
};
use crate::config::Config;
use crate::errors::ValidationError;
use crate::models::profiles::queries::get_profile_by_remote_actor_id;
use crate::models::relationships::queries::{
    follow_request_accepted,
    get_follow_request_by_id,
};
use crate::models::relationships::types::FollowRequestStatus;
use super::HandlerResult;

pub async fn handle_accept(
    config: &Config,
    db_client: &mut impl GenericClient,
    activity: Value,
) -> HandlerResult {
    // Accept(Follow)
    let activity: Activity = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let object_id = find_object_id(&activity.object)?;
    let follow_request_id = parse_local_object_id(
        &config.instance_url(),
        &object_id,
    )?;
    let follow_request = get_follow_request_by_id(db_client, &follow_request_id).await?;
    if follow_request.target_id != actor_profile.id {
        return Err(ValidationError("actor is not a target").into());
    };
    if matches!(follow_request.request_status, FollowRequestStatus::Accepted) {
        // Ignore Accept if follow request already accepted
        return Ok(None);
    };
    follow_request_accepted(db_client, &follow_request_id).await?;
    Ok(Some(FOLLOW))
}
