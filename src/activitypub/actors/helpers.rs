use std::path::Path;

use tokio_postgres::GenericClient;

use crate::activitypub::{
    actors::types::Actor,
    fetcher::fetchers::fetch_file,
    receiver::HandlerError,
};
use crate::config::Instance;
use crate::models::profiles::{
    queries::{create_profile, update_profile},
    types::{
        DbActorProfile,
        ProfileImage,
        ProfileCreateData,
        ProfileUpdateData,
    },
};

async fn fetch_actor_images(
    instance: &Instance,
    actor: &Actor,
    media_dir: &Path,
    default_avatar: Option<ProfileImage>,
    default_banner: Option<ProfileImage>,
) -> (Option<ProfileImage>, Option<ProfileImage>) {
    let maybe_avatar = if let Some(icon) = &actor.icon {
        match fetch_file(instance, &icon.url, media_dir).await {
            Ok((file_name, _)) => {
                let image = ProfileImage { file_name, media_type: None };
                Some(image)
            },
            Err(error) => {
                log::warn!("failed to fetch avatar ({})", error);
                default_avatar
            },
        }
    } else {
        None
    };
    let maybe_banner = if let Some(image) = &actor.image {
        match fetch_file(instance, &image.url, media_dir).await {
            Ok((file_name, _)) => {
                let image = ProfileImage { file_name, media_type: None };
                Some(image)
            },
            Err(error) => {
                log::warn!("failed to fetch banner ({})", error);
                default_banner
            },
        }
    } else {
        None
    };
    (maybe_avatar, maybe_banner)
}

pub async fn create_remote_profile(
    db_client: &impl GenericClient,
    instance: &Instance,
    media_dir: &Path,
    actor: Actor,
) -> Result<DbActorProfile, HandlerError> {
    let actor_address = actor.address()?;
    if actor_address.hostname == instance.hostname() {
        return Err(HandlerError::LocalObject);
    };
    let (maybe_avatar, maybe_banner) = fetch_actor_images(
        instance,
        &actor,
        media_dir,
        None,
        None,
    ).await;
    let (identity_proofs, payment_options, extra_fields) =
        actor.parse_attachments();
    let mut profile_data = ProfileCreateData {
        username: actor.preferred_username.clone(),
        hostname: Some(actor_address.hostname),
        display_name: actor.name.clone(),
        bio: actor.summary.clone(),
        avatar: maybe_avatar,
        banner: maybe_banner,
        identity_proofs,
        payment_options,
        extra_fields,
        actor_json: Some(actor),
    };
    profile_data.clean()?;
    let profile = create_profile(db_client, profile_data).await?;
    Ok(profile)
}

/// Updates remote actor's profile
pub async fn update_remote_profile(
    db_client: &impl GenericClient,
    instance: &Instance,
    media_dir: &Path,
    profile: DbActorProfile,
    actor: Actor,
) -> Result<DbActorProfile, HandlerError> {
    let actor_old = profile.actor_json.ok_or(HandlerError::LocalObject)?;
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
    let (maybe_avatar, maybe_banner) = fetch_actor_images(
        instance,
        &actor,
        media_dir,
        profile.avatar,
        profile.banner,
    ).await;
    let (identity_proofs, payment_options, extra_fields) =
        actor.parse_attachments();
    let mut profile_data = ProfileUpdateData {
        display_name: actor.name.clone(),
        bio: actor.summary.clone(),
        bio_source: actor.summary.clone(),
        avatar: maybe_avatar,
        banner: maybe_banner,
        identity_proofs,
        payment_options,
        extra_fields,
        actor_json: Some(actor),
    };
    profile_data.clean()?;
    let profile = update_profile(db_client, &profile.id, profile_data).await?;
    Ok(profile)
}
