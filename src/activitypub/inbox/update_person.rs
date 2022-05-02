use std::path::Path;

use tokio_postgres::GenericClient;

use crate::activitypub::{
    activity::Activity,
    actor::Actor,
    fetcher::fetchers::fetch_avatar_and_banner,
    fetcher::helpers::ImportError,
};
use crate::errors::ValidationError;
use crate::models::profiles::queries::{
    get_profile_by_actor_id,
    update_profile,
};
use crate::models::profiles::types::ProfileUpdateData;

pub async fn handle_update_person(
    db_client: &impl GenericClient,
    media_dir: &Path,
    activity: Activity,
) -> Result<(), ImportError> {
    let actor: Actor = serde_json::from_value(activity.object)
        .map_err(|_| ValidationError("invalid actor data"))?;
    if actor.id != activity.actor {
        return Err(ValidationError("actor ID mismatch").into());
    };
    let profile = get_profile_by_actor_id(db_client, &actor.id).await?;
    let (avatar, banner) = fetch_avatar_and_banner(&actor, media_dir).await
        .map_err(|_| ValidationError("failed to fetch image"))?;
    let (identity_proofs, extra_fields) = actor.parse_attachments();
    let actor_old = profile.actor_json.unwrap();
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
    update_profile(db_client, &profile.id, profile_data).await?;
    Ok(())
}
