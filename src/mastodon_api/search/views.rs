use actix_session::Session;
use actix_web::{get, web, HttpResponse};
use serde::Deserialize;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::users::auth::get_current_user;
use super::queries;
use super::types::SearchResults;

#[derive(Deserialize)]
struct SearchQueryParams {
    q: String,
}

#[get("/api/v2/search")]
async fn search(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    session: Session,
    query_params: web::Query<SearchQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = get_database_client(&db_pool).await?;
    get_current_user(&**db_client, session).await?;
    let profiles = queries::search(&config, &**db_client, &query_params.q).await?;
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    let results = SearchResults { accounts };
    Ok(HttpResponse::Ok().json(results))
}
