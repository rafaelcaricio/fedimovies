/// https://docs.joinmastodon.org/methods/search/
use actix_web::{get, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::oauth::auth::get_current_user;
use super::helpers::search;
use super::types::SearchQueryParams;

#[get("")]
async fn search_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    query_params: web::Query<SearchQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let results = search(
        &config,
        &current_user,
        db_client,
        query_params.q.trim(),
        query_params.limit,
    ).await?;
    Ok(HttpResponse::Ok().json(results))
}

pub fn search_api_scope() -> Scope {
    web::scope("/api/v2/search")
        .service(search_view)
}
