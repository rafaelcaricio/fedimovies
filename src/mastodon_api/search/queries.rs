use regex::Regex;
use tokio_postgres::GenericClient;

use crate::activitypub::fetcher::fetch_profile;
use crate::config::Config;
use crate::errors::{ValidationError, HttpError};
use crate::models::profiles::queries::{create_profile, search_profile};
use crate::models::profiles::types::DbActorProfile;

fn parse_search_query(query: &str) ->
    Result<(String, Option<String>), ValidationError>
{
    let acct_regexp = Regex::new(r"^@?(?P<user>\w+)(@(?P<instance>[\w\.-]+))?").unwrap();
    let acct_caps = acct_regexp.captures(query)
        .ok_or(ValidationError("invalid search query"))?;
    let username = acct_caps.name("user")
        .ok_or(ValidationError("invalid search query"))?
        .as_str().to_string();
    let instance = acct_caps.name("instance")
        .and_then(|val| Some(val.as_str().to_string()));
    Ok((username, instance))
}

pub async fn search(
    config: &Config,
    db_client: &impl GenericClient,
    search_query: &str,
) -> Result<Vec<DbActorProfile>, HttpError> {
    let (username, instance) = parse_search_query(search_query)?;
    let mut profiles = search_profile(db_client, &username, &instance).await?;
    if profiles.len() == 0 && instance.is_some() {
        let instance_uri = instance.unwrap();
        let media_dir = config.media_dir();
        let profile_data = fetch_profile(&username, &instance_uri, &media_dir).await
            .map_err(|err| {
                log::warn!("{}", err);
                HttpError::NotFoundError("remote profile")
            })?;
        let profile = create_profile(db_client, &profile_data).await?;
        log::info!(
            "imported profile '{}'",
            profile.acct,
        );
        profiles.push(profile);
    }
    Ok(profiles)
}
