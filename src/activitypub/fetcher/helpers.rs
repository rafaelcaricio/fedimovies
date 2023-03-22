use std::collections::HashMap;
use std::path::Path;

use mitra_config::{Config, Instance};

use crate::activitypub::{
    actors::helpers::{create_remote_profile, update_remote_profile},
    handlers::create::{get_object_links, handle_note},
    identifiers::parse_local_object_id,
    receiver::HandlerError,
    types::Object,
};
use crate::database::{DatabaseClient, DatabaseError};
use crate::errors::ValidationError;
use crate::models::{
    posts::helpers::get_local_post_by_id,
    posts::queries::get_post_by_remote_object_id,
    posts::types::Post,
    profiles::queries::{
        get_profile_by_acct,
        get_profile_by_remote_actor_id,
    },
    profiles::types::DbActorProfile,
};
use crate::webfinger::types::ActorAddress;
use super::fetchers::{
    fetch_actor,
    fetch_object,
    perform_webfinger_query,
    FetchError,
};

pub async fn get_or_import_profile_by_actor_id(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    media_dir: &Path,
    actor_id: &str,
) -> Result<DbActorProfile, HandlerError> {
    if actor_id.starts_with(&instance.url()) {
        return Err(HandlerError::LocalObject);
    };
    let profile = match get_profile_by_remote_actor_id(
        db_client,
        actor_id,
    ).await {
        Ok(profile) => {
            if profile.possibly_outdated() {
                // Try to re-fetch actor profile
                match fetch_actor(instance, actor_id).await {
                    Ok(actor) => {
                        log::info!("re-fetched profile {}", profile.acct);
                        let profile_updated = update_remote_profile(
                            db_client,
                            instance,
                            media_dir,
                            profile,
                            actor,
                        ).await?;
                        profile_updated
                    },
                    Err(err) => {
                        // Ignore error and return stored profile
                        log::warn!(
                            "failed to re-fetch {} ({})", profile.acct, err,
                        );
                        profile
                    },
                }
            } else {
                profile
            }
        },
        Err(DatabaseError::NotFound(_)) => {
            let actor = fetch_actor(instance, actor_id).await?;
            let actor_address = actor.address()?;
            let acct = actor_address.acct(&instance.hostname());
            match get_profile_by_acct(db_client, &acct).await {
                Ok(profile) => {
                    // WARNING: Possible actor ID change
                    log::info!("re-fetched profile {}", profile.acct);
                    let profile_updated = update_remote_profile(
                        db_client,
                        instance,
                        media_dir,
                        profile,
                        actor,
                    ).await?;
                    profile_updated
                },
                Err(DatabaseError::NotFound(_)) => {
                    log::info!("fetched profile {}", acct);
                    let profile = create_remote_profile(
                        db_client,
                        instance,
                        media_dir,
                        actor,
                    ).await?;
                    profile
                },
                Err(other_error) => return Err(other_error.into()),
            }
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(profile)
}

/// Fetches actor profile and saves it into database
pub async fn import_profile_by_actor_address(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    media_dir: &Path,
    actor_address: &ActorAddress,
) -> Result<DbActorProfile, HandlerError> {
    if actor_address.hostname == instance.hostname() {
        return Err(HandlerError::LocalObject);
    };
    let actor_id = perform_webfinger_query(instance, actor_address).await?;
    let actor = fetch_actor(instance, &actor_id).await?;
    let profile_acct = actor.address()?.acct(&instance.hostname());
    if profile_acct != actor_address.acct(&instance.hostname()) {
        // Redirected to different server
        match get_profile_by_acct(db_client, &profile_acct).await {
            Ok(profile) => return Ok(profile),
            Err(DatabaseError::NotFound(_)) => (),
            Err(other_error) => return Err(other_error.into()),
        };
    };
    log::info!("fetched profile {}", profile_acct);
    let profile = create_remote_profile(
        db_client,
        instance,
        media_dir,
        actor,
    ).await?;
    Ok(profile)
}

// Works with local profiles
pub async fn get_or_import_profile_by_actor_address(
    db_client: &mut impl DatabaseClient,
    instance: &Instance,
    media_dir: &Path,
    actor_address: &ActorAddress,
) -> Result<DbActorProfile, HandlerError> {
    let acct = actor_address.acct(&instance.hostname());
    let profile = match get_profile_by_acct(
        db_client,
        &acct,
    ).await {
        Ok(profile) => profile,
        Err(db_error @ DatabaseError::NotFound(_)) => {
            if actor_address.hostname == instance.hostname() {
                return Err(db_error.into());
            };
            import_profile_by_actor_address(
                db_client,
                instance,
                media_dir,
                actor_address,
            ).await?
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(profile)
}

pub async fn get_post_by_object_id(
    db_client: &impl DatabaseClient,
    instance_url: &str,
    object_id: &str,
) -> Result<Post, DatabaseError> {
    match parse_local_object_id(instance_url, object_id) {
        Ok(post_id) => {
            // Local post
            let post = get_local_post_by_id(db_client, &post_id).await?;
            Ok(post)
        },
        Err(_) => {
            // Remote post
            let post = get_post_by_remote_object_id(db_client, object_id).await?;
            Ok(post)
        },
    }
}

const RECURSION_DEPTH_MAX: usize = 50;

pub async fn import_post(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    object_id: String,
    object_received: Option<Object>,
) -> Result<Post, HandlerError> {
    let instance = config.instance();
    if parse_local_object_id(&instance.url(), &object_id).is_ok() {
        return Err(HandlerError::LocalObject);
    };

    let mut queue = vec![object_id]; // LIFO queue
    let mut fetch_count = 0;
    let mut maybe_object = object_received;
    let mut objects: Vec<Object> = vec![];
    let mut redirects: HashMap<String, String> = HashMap::new();
    let mut posts = vec![];

    // Fetch ancestors by going through inReplyTo references
    // TODO: fetch replies too
    #[allow(clippy::while_let_loop)]
    loop {
        let object_id = match queue.pop() {
            Some(object_id) => {
                if objects.iter().any(|object| object.id == object_id) {
                    // Can happen due to redirections
                    log::warn!("loop detected");
                    continue;
                };
                if let Ok(post_id) = parse_local_object_id(&instance.url(), &object_id) {
                    // Object is a local post
                    // Verify post exists, return error if it doesn't
                    get_local_post_by_id(db_client, &post_id).await?;
                    continue;
                };
                match get_post_by_remote_object_id(
                    db_client,
                    &object_id,
                ).await {
                    Ok(post) => {
                        // Object already fetched
                        if objects.len() == 0 {
                            // Return post corresponding to initial object ID
                            return Ok(post);
                        };
                        continue;
                    },
                    Err(DatabaseError::NotFound(_)) => (),
                    Err(other_error) => return Err(other_error.into()),
                };
                object_id
            },
            None => {
                // No object to fetch
                break;
            },
        };
        let object = match maybe_object {
            Some(object) => object,
            None => {
                if fetch_count >= RECURSION_DEPTH_MAX {
                    // TODO: create tombstone
                    return Err(FetchError::RecursionError.into());
                };
                let object = fetch_object(&instance, &object_id).await
                    .map_err(|err| {
                        log::warn!("{}", err);
                        ValidationError("failed to fetch object")
                    })?;
                log::info!("fetched object {}", object.id);
                fetch_count +=  1;
                object
            },
        };
        if object.id != object_id {
            // ID of fetched object doesn't match requested ID
            // Add IDs to the map of redirects
            redirects.insert(object_id, object.id.clone());
            queue.push(object.id.clone());
            // Don't re-fetch object on the next iteration
            maybe_object = Some(object);
            continue;
        };
        if let Some(ref object_id) = object.in_reply_to {
            // Fetch parent object on next iteration
            queue.push(object_id.to_owned());
        };
        for object_id in get_object_links(&object) {
            // Fetch linked objects after fetching current thread
            queue.insert(0, object_id);
        };
        maybe_object = None;
        objects.push(object);
    };
    let initial_object_id = objects[0].id.clone();

    // Objects are ordered according to their place in reply tree,
    // starting with the root
    objects.reverse();
    for object in objects {
        let post = handle_note(
            config,
            db_client,
            object,
            &redirects,
        ).await?;
        posts.push(post);
    };

    let initial_post = posts.into_iter()
        .find(|post| post.object_id.as_ref() == Some(&initial_object_id))
        .unwrap();
    Ok(initial_post)
}
