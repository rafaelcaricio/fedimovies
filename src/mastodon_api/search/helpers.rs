use regex::Regex;
use tokio_postgres::GenericClient;

use crate::activitypub::fetcher::fetch_profile;
use crate::config::Config;
use crate::errors::{ValidationError, HttpError};
use crate::mastodon_api::accounts::types::Account;
use crate::models::profiles::queries::{create_profile, search_profile};
use crate::models::profiles::types::DbActorProfile;
use super::types::SearchResults;

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

async fn search_profiles(
    config: &Config,
    db_client: &impl GenericClient,
    search_query: &str,
) -> Result<Vec<DbActorProfile>, HttpError> {
    let (username, instance) = parse_search_query(search_query)?;
    let mut profiles = search_profile(db_client, &username, instance.as_ref()).await?;
    if profiles.len() == 0 && instance.is_some() {
        let instance_uri = instance.unwrap();
        let media_dir = config.media_dir();
        match fetch_profile(&username, &instance_uri, &media_dir).await {
            Ok(profile_data) => {
                let profile = create_profile(db_client, &profile_data).await?;
                log::info!(
                    "imported profile '{}'",
                    profile.acct,
                );
                profiles.push(profile);
            },
            Err(err) => {
                log::warn!("{}", err);
            },
        }
    }
    Ok(profiles)
}

pub async fn search(
    config: &Config,
    db_client: &impl GenericClient,
    search_query: &str,
) -> Result<SearchResults, HttpError> {
    let profiles = search_profiles(config, db_client, search_query).await?;
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    Ok(SearchResults { accounts })
}
