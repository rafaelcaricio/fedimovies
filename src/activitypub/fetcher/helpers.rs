use std::collections::HashMap;
use std::path::Path;

use tokio_postgres::GenericClient;

use crate::activitypub::activity::Object;
use crate::activitypub::actor::ActorAddress;
use crate::activitypub::handlers::{
    create_note::handle_note,
    update_person::update_actor,
};
use crate::activitypub::receiver::parse_object_id;
use crate::config::{Config, Instance};
use crate::errors::{DatabaseError, HttpError, ValidationError};
use crate::models::posts::queries::get_post_by_object_id;
use crate::models::posts::types::Post;
use crate::models::profiles::queries::{
    get_profile_by_actor_id,
    get_profile_by_acct,
    create_profile,
};
use crate::models::profiles::types::DbActorProfile;
use super::fetchers::{
    fetch_actor,
    fetch_object,
    fetch_profile_by_actor_id,
    perform_webfinger_query,
    FetchError,
};

#[derive(thiserror::Error, Debug)]
pub enum ImportError {
    #[error("local object")]
    LocalObject,

    #[error(transparent)]
    FetchError(#[from] FetchError),

    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),
}

impl From<ImportError> for HttpError {
    fn from(error: ImportError) -> Self {
        match error {
            ImportError::LocalObject => HttpError::InternalError,
            ImportError::FetchError(error) => {
                HttpError::ValidationError(error.to_string())
            },
            ImportError::ValidationError(error) => error.into(),
            ImportError::DatabaseError(error) => error.into(),
        }
    }
}

pub async fn get_or_import_profile_by_actor_id(
    db_client: &impl GenericClient,
    instance: &Instance,
    media_dir: &Path,
    actor_id: &str,
) -> Result<DbActorProfile, ImportError> {
    if actor_id.starts_with(&instance.url()) {
        return Err(ImportError::LocalObject);
    };
    let profile = match get_profile_by_actor_id(db_client, actor_id).await {
        Ok(profile) => {
            if profile.possibly_outdated() {
                let actor = fetch_actor(instance, actor_id).await?;
                log::info!("re-fetched profile {}", profile.acct);
                let profile = update_actor(db_client, media_dir, actor).await?;
                profile
            } else {
                profile
            }
        },
        Err(DatabaseError::NotFound(_)) => {
            let mut profile_data = fetch_profile_by_actor_id(
                instance, actor_id, media_dir,
            )
                .await
                .map_err(|err| {
                    log::warn!("failed to fetch {} ({})", actor_id, err);
                    err
                })?;
            log::info!("fetched profile {}", profile_data.acct);
            profile_data.clean()?;
            let profile = create_profile(db_client, profile_data).await?;
            profile
        },
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(profile)
}

/// Fetches actor profile and saves it into database
pub async fn import_profile_by_actor_address(
    db_client: &impl GenericClient,
    instance: &Instance,
    media_dir: &Path,
    actor_address: &ActorAddress,
) -> Result<DbActorProfile, ImportError> {
    if actor_address.instance == instance.host() {
        return Err(ImportError::LocalObject);
    };
    let actor_id = perform_webfinger_query(instance, actor_address).await?;
    let mut profile_data = fetch_profile_by_actor_id(
        instance,
        &actor_id,
        media_dir,
    ).await?;
    if profile_data.acct != actor_address.acct() {
        // Redirected to different server
        match get_profile_by_acct(db_client, &profile_data.acct).await {
            Ok(profile) => return Ok(profile),
            Err(DatabaseError::NotFound(_)) => (),
            Err(other_error) => return Err(other_error.into()),
        };
    };
    log::info!("fetched profile {}", profile_data.acct);
    profile_data.clean()?;
    let profile = create_profile(db_client, profile_data).await?;
    Ok(profile)
}

pub async fn import_post(
    config: &Config,
    db_client: &mut impl GenericClient,
    object_id: String,
    object_received: Option<Object>,
) -> Result<Post, ImportError> {
    let instance = config.instance();
    let media_dir = config.media_dir();
    let mut maybe_object_id_to_fetch = Some(object_id);
    let mut maybe_object = object_received;
    let mut objects = vec![];
    let mut redirects: HashMap<String, String> = HashMap::new();
    let mut posts = vec![];

    // Fetch ancestors by going through inReplyTo references
    // TODO: fetch replies too
    #[allow(clippy::while_let_loop)]
    loop {
        let object_id = match maybe_object_id_to_fetch {
            Some(object_id) => {
                if parse_object_id(&instance.url(), &object_id).is_ok() {
                    // Object is a local post
                    assert!(objects.len() > 0);
                    break;
                }
                match get_post_by_object_id(db_client, &object_id).await {
                    Ok(post) => {
                        // Object already fetched
                        if objects.len() == 0 {
                            // Return post corresponding to initial object ID
                            return Ok(post);
                        };
                        break;
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
                let object = fetch_object(&instance, &object_id).await
                    .map_err(|err| {
                        log::warn!("{}", err);
                        ValidationError("failed to fetch object")
                    })?;
                log::info!("fetched object {}", object.id);
                object
            },
        };
        if object.id != object_id {
            // ID of fetched object doesn't match requested ID
            // Add IDs to the map of redirects
            redirects.insert(object_id, object.id.clone());
            maybe_object_id_to_fetch = Some(object.id.clone());
            // Don't re-fetch object on the next iteration
            maybe_object = Some(object);
        } else {
            maybe_object_id_to_fetch = object.in_reply_to.clone();
            maybe_object = None;
            objects.push(object);
        };
    }
    let initial_object_id = objects[0].id.clone();

    // Objects are ordered according to their place in reply tree,
    // starting with the root
    objects.reverse();
    for object in objects {
        let post = handle_note(
            db_client,
            &instance,
            &media_dir,
            object,
            &redirects,
        ).await?;
        posts.push(post);
    }

    let initial_post = posts.into_iter()
        .find(|post| post.object_id.as_ref() == Some(&initial_object_id))
        .unwrap();
    Ok(initial_post)
}
