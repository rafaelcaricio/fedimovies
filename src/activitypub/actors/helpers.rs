use std::path::Path;

use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    database::DatabaseClient,
    profiles::queries::{create_profile, update_profile},
    profiles::types::{
        DbActorProfile,
        ProfileImage,
        ProfileCreateData,
        ProfileUpdateData,
    },
};

use crate::activitypub::{
    actors::types::Actor,
    fetcher::fetchers::fetch_file,
    handlers::create::handle_emoji,
    receiver::{parse_array, HandlerError},
    vocabulary::{EMOJI, HASHTAG},
};
use crate::media::MediaStorage;
use crate::validators::{
    posts::EMOJIS_MAX_NUM,
    profiles::{clean_profile_create_data, clean_profile_update_data},
};

pub const ACTOR_IMAGE_MAX_SIZE: usize = 5 * 1000 * 1000; // 5 MB

async fn fetch_actor_images(
    instance: &Instance,
    actor: &Actor,
    media_dir: &Path,
    default_avatar: Option<ProfileImage>,
    default_banner: Option<ProfileImage>,
) -> (Option<ProfileImage>, Option<ProfileImage>) {
    let maybe_avatar = if let Some(icon) = &actor.icon {
        match fetch_file(
            instance,
            &icon.url,
            icon.media_type.as_deref(),
            ACTOR_IMAGE_MAX_SIZE,
            media_dir,
        ).await {
            Ok((file_name, file_size, maybe_media_type)) => {
                let image = ProfileImage::new(
                    file_name,
                    file_size,
                    maybe_media_type,
                );
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
        match fetch_file(
            instance,
            &image.url,
            image.media_type.as_deref(),
            ACTOR_IMAGE_MAX_SIZE,
            media_dir,
        ).await {
            Ok((file_name, file_size, maybe_media_type)) => {
                let image = ProfileImage::new(
                    file_name,
                    file_size,
                    maybe_media_type,
                );
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

fn parse_aliases(actor: &Actor) -> Vec<String> {
    // Aliases reported by server (not signed)
    actor.also_known_as.as_ref()
        .and_then(|value| {
            match parse_array(value) {
                Ok(array) => Some(array),
                Err(_) => {
                    log::warn!("invalid alias list: {}", value);
                    None
                },
            }
        })
        .unwrap_or_default()
}

async fn parse_tags(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
    actor: &Actor,
) -> Result<Vec<Uuid>, HandlerError> {
    let mut emojis = vec![];
    for tag_value in actor.tag.clone() {
        let tag_type = tag_value["type"].as_str().unwrap_or(HASHTAG);
        if tag_type == EMOJI {
            if emojis.len() >= EMOJIS_MAX_NUM {
                log::warn!("too many emojis");
                continue;
            };
            match handle_emoji(
                db_client,
                instance,
                storage,
                tag_value,
            ).await? {
                Some(emoji) => {
                    if !emojis.contains(&emoji.id) {
                        emojis.push(emoji.id);
                    };
                },
                None => continue,
            };
        } else {
            log::warn!("skipping actor tag of type {}", tag_type);
        };
    };
    Ok(emojis)
}

pub async fn create_remote_profile(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
    actor: Actor,
) -> Result<DbActorProfile, HandlerError> {
    let actor_address = actor.address()?;
    if actor_address.hostname == instance.hostname() {
        return Err(HandlerError::LocalObject);
    };
    let (maybe_avatar, maybe_banner) = fetch_actor_images(
        instance,
        &actor,
        &storage.media_dir,
        None,
        None,
    ).await;
    let (identity_proofs, payment_options, extra_fields) =
        actor.parse_attachments();
    let aliases = parse_aliases(&actor);
    let emojis = parse_tags(
        db_client,
        instance,
        storage,
        &actor,
    ).await?;
    let mut profile_data = ProfileCreateData {
        username: actor.preferred_username.clone(),
        hostname: Some(actor_address.hostname),
        display_name: actor.name.clone(),
        bio: actor.summary.clone(),
        avatar: maybe_avatar,
        banner: maybe_banner,
        manually_approves_followers: actor.manually_approves_followers,
        identity_proofs,
        payment_options,
        extra_fields,
        aliases,
        emojis,
        actor_json: Some(actor.into_db_actor()),
    };
    clean_profile_create_data(&mut profile_data)?;
    let profile = create_profile(db_client, profile_data).await?;
    Ok(profile)
}

/// Updates remote actor's profile
pub async fn update_remote_profile(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    storage: &MediaStorage,
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
        &storage.media_dir,
        profile.avatar,
        profile.banner,
    ).await;
    let (identity_proofs, payment_options, extra_fields) =
        actor.parse_attachments();
    let aliases = parse_aliases(&actor);
    let emojis = parse_tags(
        db_client,
        instance,
        storage,
        &actor,
    ).await?;
    let mut profile_data = ProfileUpdateData {
        display_name: actor.name.clone(),
        bio: actor.summary.clone(),
        bio_source: actor.summary.clone(),
        avatar: maybe_avatar,
        banner: maybe_banner,
        manually_approves_followers: actor.manually_approves_followers,
        identity_proofs,
        payment_options,
        extra_fields,
        aliases,
        emojis,
        actor_json: Some(actor.into_db_actor()),
    };
    clean_profile_update_data(&mut profile_data)?;
    let profile = update_profile(db_client, &profile.id, profile_data).await?;
    Ok(profile)
}
