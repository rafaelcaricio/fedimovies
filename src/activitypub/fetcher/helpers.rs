use std::path::Path;

use tokio_postgres::GenericClient;

use crate::activitypub::actor::ActorAddress;
use crate::config::Instance;
use crate::errors::{DatabaseError, HttpError, ValidationError};
use crate::models::profiles::queries::{
    get_profile_by_actor_id,
    get_profile_by_acct,
    create_profile,
};
use crate::models::profiles::types::DbActorProfile;
use super::fetchers::{
    fetch_profile,
    fetch_profile_by_actor_id,
    FetchError,
};

#[derive(thiserror::Error, Debug)]
pub enum ImportError {
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
            ImportError::FetchError(error) => {
                HttpError::ValidationError(error.to_string())
            },
            ImportError::ValidationError(error) => error.into(),
            ImportError::DatabaseError(error) => error.into(),
        }
    }
}

pub async fn get_or_fetch_profile_by_actor_id(
    db_client: &impl GenericClient,
    instance: &Instance,
    actor_id: &str,
    media_dir: &Path,
) -> Result<DbActorProfile, HttpError> {
    let profile = match get_profile_by_actor_id(db_client, actor_id).await {
        Ok(profile) => profile,
        Err(DatabaseError::NotFound(_)) => {
            let profile_data = fetch_profile_by_actor_id(
                instance, actor_id, media_dir,
            )
                .await
                .map_err(|err| {
                    log::warn!("{}", err);
                    ValidationError("failed to fetch actor")
                })?;
            log::info!("fetched profile {}", profile_data.acct);
            let profile = create_profile(db_client, &profile_data).await?;
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
    let profile_data = fetch_profile(
        instance,
        &actor_address.username,
        &actor_address.instance,
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
    let profile = create_profile(db_client, &profile_data).await?;
    Ok(profile)
}
