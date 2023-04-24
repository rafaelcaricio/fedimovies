/// https://docs.joinmastodon.org/methods/search/
use actix_web::{dev::ConnectionInfo, get, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_config::Config;
use mitra_models::database::{get_database_client, DbPool};

use super::helpers::search;
use super::types::{SearchQueryParams, SearchResults};
use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::types::Account, errors::MastodonError, oauth::auth::get_current_user,
    statuses::helpers::build_status_list, statuses::types::Tag,
};

#[get("")]
async fn search_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<SearchQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let (profiles, posts, tags) = search(
        &config,
        &current_user,
        db_client,
        query_params.q.trim(),
        query_params.limit.inner(),
    )
    .await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = profiles
        .into_iter()
        .map(|profile| Account::from_profile(&base_url, &instance_url, profile))
        .collect();
    let statuses = build_status_list(
        db_client,
        &base_url,
        &instance_url,
        Some(&current_user),
        posts,
    )
    .await?;
    let hashtags = tags
        .into_iter()
        .map(|tag_name| Tag::from_tag_name(&instance_url, tag_name))
        .collect();
    let results = SearchResults {
        accounts,
        statuses,
        hashtags,
    };
    Ok(HttpResponse::Ok().json(results))
}

pub fn search_api_scope() -> Scope {
    web::scope("/api/v2/search").service(search_view)
}
