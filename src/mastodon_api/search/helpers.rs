use std::str::FromStr;

use regex::Regex;
use url::Url;

use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::{
        helpers::{can_view_post, get_local_post_by_id},
        types::Post,
    },
    profiles::queries::{
        search_profiles,
        search_profiles_by_did_only,
        search_profiles_by_wallet_address,
    },
    profiles::types::DbActorProfile,
    tags::queries::search_tags,
    users::{
        queries::get_user_by_name,
        types::User,
    },
};
use mitra_utils::{
    currencies::Currency,
    did::Did,
};

use crate::activitypub::{
    fetcher::helpers::{
        get_or_import_profile_by_actor_id,
        import_post,
        import_profile_by_actor_address,
    },
    identifiers::{parse_local_actor_id, parse_local_object_id},
    HandlerError,
};
use crate::errors::ValidationError;
use crate::ethereum::utils::validate_ethereum_address;
use crate::media::MediaStorage;
use crate::webfinger::types::ActorAddress;

const SEARCH_FETCHER_TIMEOUT: u64 = 5;

enum SearchQuery {
    ProfileQuery(String, Option<String>),
    TagQuery(String),
    Url(String),
    WalletAddress(String),
    Did(Did),
    Unknown,
}

fn parse_profile_query(query: &str) ->
    Result<(String, Option<String>), ValidationError>
{
    // See also: ACTOR_ADDRESS_RE in webfinger::types
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
    if let Ok(did) = Did::from_str(search_query) {
        return SearchQuery::Did(did);
    };
    if Url::parse(search_query).is_ok() {
        return SearchQuery::Url(search_query.to_string());
    };
    // TODO: support other currencies
    if validate_ethereum_address(
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
    db_client: &mut impl DatabaseClient,
    username: String,
    mut maybe_hostname: Option<String>,
    resolve: bool,
    limit: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let mut instance = config.instance();
    if let Some(ref hostname) = maybe_hostname {
        if hostname == &instance.hostname() {
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
    if profiles.is_empty() && resolve {
        if let Some(hostname) = maybe_hostname {
            let actor_address = ActorAddress { username, hostname };
            instance.fetcher_timeout = SEARCH_FETCHER_TIMEOUT;
            match import_profile_by_actor_address(
                db_client,
                &instance,
                &MediaStorage::from(config),
                &actor_address,
            ).await {
                Ok(profile) => {
                    profiles.push(profile);
                },
                Err(HandlerError::DatabaseError(db_error)) => {
                    // Propagate database errors
                    return Err(db_error);
                },
                Err(other_error) => {
                    log::warn!(
                        "failed to import profile {}: {}",
                        actor_address,
                        other_error,
                    );
                },
            };
        };
    };
    Ok(profiles)
}

/// Finds post by its object ID
async fn find_post_by_url(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    url: &str,
) -> Result<Option<Post>, DatabaseError> {
    let mut instance = config.instance();
    let storage = MediaStorage::from(config);
    let maybe_post = match parse_local_object_id(&instance.url(), url) {
        Ok(post_id) => {
            // Local URL
            match get_local_post_by_id(db_client, &post_id).await {
                Ok(post) => Some(post),
                Err(DatabaseError::NotFound(_)) => None,
                Err(other_error) => return Err(other_error),
            }
        },
        Err(_) => {
            instance.fetcher_timeout = SEARCH_FETCHER_TIMEOUT;
            match import_post(
                db_client,
                &instance,
                &storage,
                url.to_string(),
                None,
            ).await {
                Ok(post) => Some(post),
                Err(err) => {
                    log::warn!("{}", err);
                    None
                },
            }
        },
    };
    Ok(maybe_post)
}

async fn find_profile_by_url(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    url: &str,
) -> Result<Option<DbActorProfile>, DatabaseError> {
    let mut instance = config.instance();
    let profile = match parse_local_actor_id(&instance.url(), url) {
        Ok(username) => {
            // Local URL
            match get_user_by_name(db_client, &username).await {
                Ok(user) => Some(user.profile),
                Err(DatabaseError::NotFound(_)) => None,
                Err(other_error) => return Err(other_error),
            }
        },
        Err(_) => {
            instance.fetcher_timeout = SEARCH_FETCHER_TIMEOUT;
            get_or_import_profile_by_actor_id(
                db_client,
                &instance,
                &MediaStorage::from(config),
                url,
            ).await
                .map_err(|err| log::warn!("{}", err))
                .ok()
        },
    };
    Ok(profile)
}

type SearchResults = (Vec<DbActorProfile>, Vec<Post>, Vec<String>);

pub async fn search(
    config: &Config,
    current_user: &User,
    db_client: &mut impl DatabaseClient,
    search_query: &str,
    limit: u16,
) -> Result<SearchResults, DatabaseError> {
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
                true,
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
            let maybe_post = find_post_by_url(config, db_client, &url).await?;
            if let Some(post) = maybe_post {
                if can_view_post(db_client, Some(current_user), &post).await? {
                    posts = vec![post];
                };
            } else {
                let maybe_profile = find_profile_by_url(
                    config,
                    db_client,
                    &url,
                ).await?;
                if let Some(profile) = maybe_profile {
                    profiles = vec![profile];
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
            profiles = search_profiles_by_did_only(
                db_client,
                &did,
            ).await?;
        },
        SearchQuery::Unknown => (), // ignore
    };
    Ok((profiles, posts, tags))
}

pub async fn search_profiles_only(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    search_query: &str,
    resolve: bool,
    limit: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let (username, maybe_hostname) = match parse_profile_query(search_query) {
        Ok(result) => result,
        Err(_) => return Ok(vec![]),
    };
    let profiles = search_profiles_or_import(
        config,
        db_client,
        username,
        maybe_hostname,
        resolve,
        limit,
    ).await?;
    Ok(profiles)
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
