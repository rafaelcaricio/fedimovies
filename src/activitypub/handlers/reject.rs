use serde::Deserialize;
use serde_json::Value;

use mitra_config::Config;
use mitra_models::{
    database::DatabaseClient,
    profiles::queries::get_profile_by_remote_actor_id,
    relationships::queries::{
        follow_request_rejected,
        get_follow_request_by_id,
    },
    relationships::types::FollowRequestStatus,
};

use crate::activitypub::{
    identifiers::parse_local_object_id,
    receiver::deserialize_into_object_id,
    vocabulary::FOLLOW,
};
use crate::errors::ValidationError;

use super::HandlerResult;

#[derive(Deserialize)]
struct Reject {
    actor: String,
    #[serde(deserialize_with = "deserialize_into_object_id")]
    object: String,
}

pub async fn handle_reject(
    config: &Config,
    db_client: &impl DatabaseClient,
    activity: Value,
) -> HandlerResult {
    // Reject(Follow)
    let activity: Reject = serde_json::from_value(activity)
        .map_err(|_| ValidationError("unexpected activity structure"))?;
    let actor_profile = get_profile_by_remote_actor_id(
        db_client,
        &activity.actor,
    ).await?;
    let follow_request_id = parse_local_object_id(
        &config.instance_url(),
        &activity.object,
    )?;
    let follow_request = get_follow_request_by_id(db_client, &follow_request_id).await?;
    if follow_request.target_id != actor_profile.id {
        return Err(ValidationError("actor is not a target").into());
    };
    if matches!(follow_request.request_status, FollowRequestStatus::Rejected) {
        // Ignore Reject if follow request already rejected
        return Ok(None);
    };
    follow_request_rejected(db_client, &follow_request_id).await?;
    Ok(Some(FOLLOW))
}
