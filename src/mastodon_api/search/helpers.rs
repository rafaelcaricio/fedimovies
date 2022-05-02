use regex::Regex;
use tokio_postgres::GenericClient;
use url::Url;

use crate::activitypub::actor::ActorAddress;
use crate::activitypub::fetcher::helpers::{
    import_post,
    import_profile_by_actor_address,
};
use crate::config::Config;
use crate::errors::{ValidationError, HttpError};
use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::statuses::types::Status;
use crate::models::posts::helpers::can_view_post;
use crate::models::posts::types::Post;
use crate::models::profiles::queries::{
    search_profile,
    search_profile_by_wallet_address,
};
use crate::models::profiles::types::DbActorProfile;
use crate::models::users::types::{
    validate_wallet_address,
    User,
};
use super::types::SearchResults;

enum SearchQuery {
    ProfileQuery(String, Option<String>),
    Url(String),
    WalletAddress(String),
    Unknown,
}

fn parse_profile_query(query: &str) ->
    Result<(String, Option<String>), ValidationError>
{
    // See also: USERNAME_RE in models::profiles::validators
    let acct_regexp = Regex::new(r"^@?(?P<user>[\w\.-]+)(@(?P<instance>[\w\.-]+))?$").unwrap();
    let acct_caps = acct_regexp.captures(query)
        .ok_or(ValidationError("invalid search query"))?;
    let username = acct_caps.name("user")
        .ok_or(ValidationError("invalid search query"))?
        .as_str().to_string();
    let maybe_instance = acct_caps.name("instance")
        .map(|val| val.as_str().to_string());
    Ok((username, maybe_instance))
}

fn parse_search_query(search_query: &str) -> SearchQuery {
    let search_query = search_query.trim();
    if Url::parse(search_query).is_ok() {
        return SearchQuery::Url(search_query.to_string());
    };
    if validate_wallet_address(&search_query.to_lowercase()).is_ok() {
        return SearchQuery::WalletAddress(search_query.to_string());
    };
    match parse_profile_query(search_query) {
        Ok((username, instance)) => {
            SearchQuery::ProfileQuery(username, instance)
        },
        Err(_) => {
            SearchQuery::Unknown
        },
    }
}

async fn search_profiles(
    config: &Config,
    db_client: &impl GenericClient,
    username: String,
    mut instance: Option<String>,
) -> Result<Vec<DbActorProfile>, HttpError> {
    if let Some(ref actor_host) = instance {
        if actor_host == &config.instance().host() {
            // This is a local profile
            instance = None;
        };
    };
    let mut profiles = search_profile(db_client, &username, instance.as_ref()).await?;
    if profiles.is_empty() && instance.is_some() {
        let actor_address = ActorAddress {
            username: username,
            instance: instance.unwrap(),
            is_local: false,
        };
        match import_profile_by_actor_address(
            db_client,
            &config.instance(),
            &config.media_dir(),
            &actor_address,
        ).await {
            Ok(profile) => {
                profiles.push(profile);
            },
            Err(err) => {
                log::warn!("{}", err);
            },
        }
    }
    Ok(profiles)
}

/// Finds public post by its object ID
async fn search_post(
    config: &Config,
    db_client: &mut impl GenericClient,
    url: String,
) -> Result<Option<Post>, HttpError> {
    let maybe_post = match import_post(
        config, db_client,
        url,
        None,
    ).await {
        Ok(post) => Some(post),
        Err(err) => {
            log::warn!("{}", err);
            None
        },
    };
    Ok(maybe_post)
}

pub async fn search(
    config: &Config,
    current_user: &User,
    db_client: &mut impl GenericClient,
    search_query: &str,
) -> Result<SearchResults, HttpError> {
    let mut profiles = vec![];
    let mut posts = vec![];
    match parse_search_query(search_query) {
        SearchQuery::ProfileQuery(username, instance) => {
            profiles = search_profiles(config, db_client, username, instance).await?;
        },
        SearchQuery::Url(url) => {
            let maybe_post = search_post(config, db_client, url).await?;
            if let Some(post) = maybe_post {
                if can_view_post(db_client, Some(current_user), &post).await? {
                    posts = vec![post];
                };
            };
        },
        SearchQuery::WalletAddress(address) => {
            // Search by wallet address, assuming default currency (ethereum)
            // TODO: support other currencies
            profiles = search_profile_by_wallet_address(
                db_client,
                &config.default_currency(),
                &address,
                false,
            ).await?;
        },
        SearchQuery::Unknown => (), // ignore
    };
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    let statuses: Vec<Status> = posts.into_iter()
        .map(|post| Status::from_post(post, &config.instance_url()))
        .collect();
    Ok(SearchResults { accounts, statuses })
}
