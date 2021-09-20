use regex::Regex;
use serde_json::Value;
use uuid::Uuid;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{HttpError, ValidationError};
use crate::models::attachments::queries::create_attachment;
use crate::models::posts::types::PostCreateData;
use crate::models::posts::queries::create_post;
use crate::models::profiles::queries::{
    get_profile_by_actor_id,
    get_profile_by_acct,
    update_profile,
};
use crate::models::profiles::types::ProfileUpdateData;
use crate::models::relationships::queries::{
    follow_request_accepted,
    follow_request_rejected,
    follow, unfollow,
};
use crate::models::users::queries::get_user_by_id;
use super::activity::{Object, Activity, create_activity_accept_follow};
use super::actor::Actor;
use super::deliverer::deliver_activity;
use super::fetcher::{fetch_avatar_and_banner, fetch_attachment};
use super::vocabulary::*;

fn parse_actor_id(actor_id: &str) -> Result<String, ValidationError> {
    let url_regexp = Regex::new(r"^https?://.+/users/(?P<username>[0-9a-z_]+)$").unwrap();
    let url_caps = url_regexp.captures(&actor_id)
        .ok_or(ValidationError("invalid actor ID"))?;
    let username = url_caps.name("username")
        .ok_or(ValidationError("invalid actor ID"))?
        .as_str()
        .to_owned();
    Ok(username)
}

fn parse_object_id(object_id: &str) -> Result<Uuid, ValidationError> {
    let url_regexp = Regex::new(r"^https?://.+/objects/(?P<uuid>[0-9a-f-]+)$").unwrap();
    let url_caps = url_regexp.captures(&object_id)
        .ok_or(ValidationError("invalid object ID"))?;
    let object_uuid: Uuid = url_caps.name("uuid")
        .ok_or(ValidationError("invalid object ID"))?
        .as_str().parse()
        .map_err(|_| ValidationError("invalid object ID"))?;
    Ok(object_uuid)
}

pub async fn receive_activity(
    config: &Config,
    db_pool: &Pool,
    _username: String,
    activity_raw: Value,
) -> Result<(), HttpError> {
    let activity: Activity = serde_json::from_value(activity_raw)
        .map_err(|_| ValidationError("invalid activity"))?;
    let activity_type = activity.activity_type;
    let object_type = activity.object.get("type")
        .and_then(|val| val.as_str())
        .unwrap_or("Unknown")
        .to_owned();
    let db_client = &mut **get_database_client(&db_pool).await?;
    match (activity_type.as_str(), object_type.as_str()) {
        (ACCEPT, FOLLOW) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            // TODO: reject if object ID contains wrong instance URI
            let follow_request_id = parse_object_id(&object.id)?;
            follow_request_accepted(db_client, &follow_request_id).await?;
        },
        (REJECT, FOLLOW) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            let follow_request_id = parse_object_id(&object.id)?;
            follow_request_rejected(db_client, &follow_request_id).await?;
        },
        (CREATE, NOTE) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            let attributed_to = object.attributed_to
                .ok_or(ValidationError("unattributed note"))?;
            let author = get_profile_by_actor_id(db_client, &attributed_to).await?;
            let content = object.content
                .ok_or(ValidationError("no content"))?;
            let mut attachments: Vec<Uuid> = Vec::new();
            if let Some(list) = object.attachment {
                let mut downloaded: Vec<(String, String)> = Vec::new();
                let output_dir = config.media_dir();
                for attachment in list {
                    let file_name = fetch_attachment(&attachment.url, &output_dir).await
                        .map_err(|_| ValidationError("failed to fetch attachment"))?;
                    log::info!("downloaded attachment {}", attachment.url);
                    downloaded.push((file_name, attachment.media_type));
                }
                for (file_name, media_type) in downloaded {
                    let db_attachment = create_attachment(
                        db_client,
                        &author.id,
                        Some(media_type),
                        file_name,
                    ).await?;
                    attachments.push(db_attachment.id);
                }
            }
            let post_data = PostCreateData {
                content,
                attachments: attachments,
                created_at: object.published,
            };
            create_post(db_client, &author.id, post_data).await?;
        },
        (FOLLOW, _) => {
            let source_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
            let source_actor_value = source_profile.actor_json.ok_or(HttpError::InternalError)?;
            let source_actor: Actor = serde_json::from_value(source_actor_value)
                .map_err(|_| HttpError::InternalError)?;
            let target_actor_id = activity.object.as_str()
                .ok_or(ValidationError("invalid object"))?;
            // TODO: reject if object ID contains wrong instance URI
            let target_username = parse_actor_id(&target_actor_id)?;
            let target_profile = get_profile_by_acct(db_client, &target_username).await?;
            // Create and send 'Accept' activity
            let target_user = get_user_by_id(db_client, &target_profile.id).await?;
            let new_activity = create_activity_accept_follow(&config, &target_profile, &activity.id);
            // Save relationship
            follow(db_client, &source_profile.id, &target_profile.id).await?;

            // Send activity
            let recipients = vec![source_actor];
            let config_clone = config.clone();
            actix_rt::spawn(async move {
                deliver_activity(
                    &config_clone,
                    &target_user,
                    new_activity,
                    recipients,
                ).await;
            });
        },
        (UNDO, FOLLOW) => {
            let object: Object = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid object"))?;
            let source_profile = get_profile_by_actor_id(db_client, &activity.actor).await?;
            let target_actor_id = object.object
                .ok_or(ValidationError("invalid object"))?;
            // TODO: reject if actor ID contains wrong instance URI
            let target_username = parse_actor_id(&target_actor_id)?;
            let target_profile = get_profile_by_acct(db_client, &target_username).await?;
            unfollow(db_client, &source_profile.id, &target_profile.id).await?;
        },
        (UPDATE, PERSON) => {
            let actor: Actor = serde_json::from_value(activity.object)
                .map_err(|_| ValidationError("invalid actor data"))?;
            let profile = get_profile_by_actor_id(db_client, &actor.id).await?;
            let (avatar, banner) = fetch_avatar_and_banner(&actor, &config.media_dir()).await
                .map_err(|_| ValidationError("failed to fetch image"))?;
            let extra_fields = actor.extra_fields();
            let mut profile_data = ProfileUpdateData {
                display_name: Some(actor.name),
                bio: actor.summary.clone(),
                bio_source: actor.summary,
                avatar,
                banner,
                extra_fields,
            };
            profile_data.clean()?;
            update_profile(db_client, &profile.id, profile_data).await?;
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
    use super::*;

    #[test]
    fn test_parse_actor_id() {
        let username = parse_actor_id("https://example.org/users/test").unwrap();
        assert_eq!(username, "test".to_string());
    }

    #[test]
    fn test_parse_actor_id_wrong_path() {
        let error = parse_actor_id("https://example.org/user/test").unwrap_err();
        assert_eq!(error.to_string(), "invalid actor ID");
    }

    #[test]
    fn test_parse_actor_id_invalid_username() {
        let error = parse_actor_id("https://example.org/users/tes-t").unwrap_err();
        assert_eq!(error.to_string(), "invalid actor ID");
    }

    #[test]
    fn test_parse_object_id() {
        let expected_uuid = Uuid::new_v4();
        let object_id = format!(
            "https://example.org/objects/{}",
            expected_uuid,
        );
        let object_uuid = parse_object_id(&object_id).unwrap();
        assert_eq!(object_uuid, expected_uuid);
    }

    #[test]
    fn test_parse_object_id_invalid_uuid() {
        let error = parse_object_id("https://example.org/objects/1234").unwrap_err();
        assert_eq!(error.to_string(), "invalid object ID");
    }
}
