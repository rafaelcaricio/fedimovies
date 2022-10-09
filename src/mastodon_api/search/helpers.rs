use std::str::FromStr;

use regex::Regex;
use tokio_postgres::GenericClient;
use url::Url;

use crate::activitypub::actors::types::ActorAddress;
use crate::activitypub::fetcher::helpers::{
    import_post,
    import_profile_by_actor_address,
};
use crate::config::Config;
use crate::errors::{ValidationError, HttpError};
use crate::ethereum::identity::DidPkh;
use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::statuses::helpers::build_status_list;
use crate::mastodon_api::statuses::types::Tag;
use crate::models::posts::helpers::can_view_post;
use crate::models::posts::types::Post;
use crate::models::profiles::queries::{
    search_profiles,
    search_profiles_by_did,
    search_profiles_by_wallet_address,
};
use crate::models::profiles::types::DbActorProfile;
use crate::models::tags::queries::search_tags;
use crate::models::users::types::User;
use crate::utils::currencies::{validate_wallet_address, Currency};
use super::types::SearchResults;

enum SearchQuery {
    ProfileQuery(String, Option<String>),
    TagQuery(String),
    Url(String),
    WalletAddress(String),
    Did(DidPkh),
    Unknown,
}

fn parse_profile_query(query: &str) ->
    Result<(String, Option<String>), ValidationError>
{
    // See also: ACTOR_ADDRESS_RE in activitypub::actors::types
    let acct_query_re =
        Regex::new(r"^(@|!)?(?P<username>[\w\.-]+)(@(?P<hostname>[\w\.-]+))?$").unwrap();
    let acct_query_caps = acct_query_re.captures(query)
        .ok_or(ValidationError("invalid profile query"))?;
    let username = acct_query_caps.name("username")
        .ok_or(ValidationError("invalid profile query"))?
        .as_str().to_string();
    let maybe_hostname = acct_query_caps.name("hostname")
        .map(|val| val.as_str().to_string());
    Ok((username, maybe_hostname))
}

fn parse_tag_query(query: &str) -> Result<String, ValidationError> {
    let tag_query_re = Regex::new(r"^#(?P<tag>\w+)$").unwrap();
    let tag_query_caps = tag_query_re.captures(query)
        .ok_or(ValidationError("invalid tag query"))?;
    let tag = tag_query_caps.name("tag")
        .ok_or(ValidationError("invalid tag query"))?
        .as_str().to_string();
    Ok(tag)
}

fn parse_search_query(search_query: &str) -> SearchQuery {
    let search_query = search_query.trim();
    // DID is a valid URI so it should be tried before Url::parse
    if let Ok(did) = DidPkh::from_str(search_query) {
        return SearchQuery::Did(did);
    };
    if Url::parse(search_query).is_ok() {
        return SearchQuery::Url(search_query.to_string());
    };
    // TODO: support other currencies
    if validate_wallet_address(
        &Currency::Ethereum,
        &search_query.to_lowercase(),
    ).is_ok() {
        return SearchQuery::WalletAddress(search_query.to_string());
    };
    if let Ok(tag) = parse_tag_query(search_query) {
        return SearchQuery::TagQuery(tag);
    };
    if let Ok((username, maybe_hostname)) = parse_profile_query(search_query) {
        return SearchQuery::ProfileQuery(username, maybe_hostname);
    };
    SearchQuery::Unknown
}

async fn search_profiles_or_import(
    config: &Config,
    db_client: &impl GenericClient,
    username: String,
    mut maybe_hostname: Option<String>,
    limit: u16,
) -> Result<Vec<DbActorProfile>, HttpError> {
    if let Some(ref hostname) = maybe_hostname {
        if hostname == &config.instance().host() {
            // This is a local profile
            maybe_hostname = None;
        };
    };
    let mut profiles = search_profiles(
        db_client,
        &username,
        maybe_hostname.as_ref(),
        limit,
    ).await?;
    if profiles.is_empty() && maybe_hostname.is_some() {
        let actor_address = ActorAddress {
            username: username,
            hostname: maybe_hostname.unwrap(),
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
    limit: u16,
) -> Result<SearchResults, HttpError> {
    let mut profiles = vec![];
    let mut posts = vec![];
    let mut tags = vec![];
    match parse_search_query(search_query) {
        SearchQuery::ProfileQuery(username, maybe_hostname) => {
            profiles = search_profiles_or_import(
                config,
                db_client,
                username,
                maybe_hostname,
                limit,
            ).await?;
        },
        SearchQuery::TagQuery(tag) => {
            tags = search_tags(
                db_client,
                &tag,
                limit,
            ).await?;
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
            // Search by wallet address, assuming it's ethereum address
            // TODO: support other currencies
            profiles = search_profiles_by_wallet_address(
                db_client,
                &Currency::Ethereum,
                &address,
                false,
            ).await?;
        },
        SearchQuery::Did(did) => {
            profiles = search_profiles_by_did(
                db_client,
                &did,
                false,
            ).await?;
        },
        SearchQuery::Unknown => (), // ignore
    };
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    let statuses = build_status_list(
        db_client,
        &config.instance_url(),
        Some(current_user),
        posts,
    ).await?;
    let hashtags = tags.into_iter()
        .map(Tag::from_tag_name)
        .collect();
    Ok(SearchResults { accounts, statuses, hashtags })
}

pub async fn search_profiles_only(
    config: &Config,
    db_client: &impl GenericClient,
    search_query: &str,
    limit: u16,
) -> Result<Vec<Account>, HttpError> {
    let (username, maybe_hostname) = match parse_profile_query(search_query) {
        Ok(result) => result,
        Err(_) => return Ok(vec![]),
    };
    let profiles = search_profiles(
        db_client,
        &username,
        maybe_hostname.as_ref(),
        limit,
    ).await?;
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_profile_query() {
        let query = "@user";
        let (username, maybe_hostname) = parse_profile_query(query).unwrap();
        assert_eq!(username, "user");
        assert_eq!(maybe_hostname, None);
    }

    #[test]
    fn test_parse_profile_query_group() {
        let query = "!group@example.com";
        let (username, maybe_hostname) = parse_profile_query(query).unwrap();
        assert_eq!(username, "group");
        assert_eq!(maybe_hostname.as_deref(), Some("example.com"));
    }

    #[test]
    fn test_parse_tag_query() {
        let query = "#Activity";
        let tag = parse_tag_query(query).unwrap();

        assert_eq!(tag, "Activity");
    }
}
