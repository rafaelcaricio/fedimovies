use regex::Regex;
use tokio_postgres::GenericClient;

use crate::activitypub::fetcher::{fetch_object, fetch_profile};
use crate::activitypub::receiver::process_note;
use crate::config::Config;
use crate::errors::{ValidationError, HttpError};
use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::statuses::types::Status;
use crate::models::posts::types::Post;
use crate::models::profiles::queries::{create_profile, search_profile};
use crate::models::profiles::types::DbActorProfile;
use super::types::SearchResults;

fn parse_profile_query(query: &str) ->
    Result<(String, Option<String>), ValidationError>
{
    let acct_regexp = Regex::new(r"^@?(?P<user>\w+)(@(?P<instance>[\w\.-]+))?$").unwrap();
    let acct_caps = acct_regexp.captures(query)
        .ok_or(ValidationError("invalid search query"))?;
    let username = acct_caps.name("user")
        .ok_or(ValidationError("invalid search query"))?
        .as_str().to_string();
    let maybe_instance = acct_caps.name("instance")
        .map(|val| val.as_str().to_string());
    Ok((username, maybe_instance))
}

async fn search_profiles(
    config: &Config,
    db_client: &impl GenericClient,
    search_query: &str,
) -> Result<Vec<DbActorProfile>, HttpError> {
    let (username, instance) = match parse_profile_query(search_query) {
        Ok((username, mut instance)) => {
            if let Some(ref actor_host) = instance {
                if actor_host == &config.instance().host() {
                    // This is a local profile
                    instance = None;
                };
            };
            (username, instance)
        },
        Err(_) => {
            // Not an 'acct' query
            return Ok(vec![]);
        },
    };
    let mut profiles = search_profile(db_client, &username, instance.as_ref()).await?;
    if profiles.is_empty() && instance.is_some() {
        let actor_host = instance.unwrap();
        let media_dir = config.media_dir();
        // TODO: return error when trying to fetch local profile
        match fetch_profile(&username, &actor_host, &media_dir).await {
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

async fn search_note(
    config: &Config,
    db_client: &mut impl GenericClient,
    search_query: &str,
) -> Result<Option<Post>, HttpError> {
    if url::Url::parse(search_query).is_err() {
        // Not a valid URL
        return Ok(None);
    }
    let maybe_post = if let Ok(object) = fetch_object(search_query).await {
        let post = process_note(config, db_client, object).await?;
        Some(post)
    } else {
        None
    };
    Ok(maybe_post)
}

pub async fn search(
    config: &Config,
    db_client: &mut impl GenericClient,
    search_query: &str,
) -> Result<SearchResults, HttpError> {
    let profiles = search_profiles(config, db_client, search_query).await?;
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    let maybe_post = search_note(config, db_client, search_query).await?;
    let statuses = match maybe_post {
        Some(post) => {
            let status = Status::from_post(post, &config.instance_url());
            vec![status]
        },
        None => vec![],
    };
    Ok(SearchResults { accounts, statuses })
}
