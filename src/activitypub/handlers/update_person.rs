use std::path::Path;

use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    actors::types::Actor,
    fetcher::fetchers::{fetch_actor_avatar, fetch_actor_banner},
    fetcher::helpers::ImportError,
    vocabulary::PERSON,
};
use crate::errors::ValidationError;
use crate::models::profiles::queries::{
    get_profile_by_actor_id,
    update_profile,
};
use crate::models::profiles::types::{DbActorProfile, ProfileUpdateData};
use super::HandlerResult;

pub async fn handle_update_person(
    db_client: &impl GenericClient,
    media_dir: &Path,
    activity: Activity,
) -> HandlerResult {
    let actor: Actor = serde_json::from_value(activity.object)
        .map_err(|_| ValidationError("invalid actor data"))?;
    if actor.id != activity.actor {
        return Err(ValidationError("actor ID mismatch").into());
    };
    let profile = get_profile_by_actor_id(db_client, &actor.id).await?;
    update_remote_profile(db_client, media_dir, profile, actor).await?;
    Ok(Some(PERSON))
}

/// Updates remote actor's profile
pub async fn update_remote_profile(
    db_client: &impl GenericClient,
    media_dir: &Path,
    profile: DbActorProfile,
    actor: Actor,
) -> Result<DbActorProfile, ImportError> {
    let actor_old = profile.actor_json.ok_or(ImportError::LocalObject)?;
    if actor_old.id != actor.id {
        log::warn!(
            "actor ID changed from {} to {}",
            actor_old.id,
            actor.id,
        );
    };
    if actor_old.public_key.public_key_pem != actor.public_key.public_key_pem {
        log::warn!(
            "actor public key changed from {} to {}",
            actor_old.public_key.public_key_pem,
            actor.public_key.public_key_pem,
        );
    };
    let avatar = fetch_actor_avatar(&actor, media_dir, profile.avatar_file_name).await;
    let banner = fetch_actor_banner(&actor, media_dir, profile.banner_file_name).await;
    let (identity_proofs, extra_fields) = actor.parse_attachments();
    let mut profile_data = ProfileUpdateData {
        display_name: actor.name.clone(),
        bio: actor.summary.clone(),
        bio_source: actor.summary.clone(),
        avatar,
        banner,
        identity_proofs,
        extra_fields,
        actor_json: Some(actor),
    };
    profile_data.clean()?;
    let profile = update_profile(db_client, &profile.id, profile_data).await?;
    Ok(profile)
}
