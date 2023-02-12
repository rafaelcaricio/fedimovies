/// https://docs.joinmastodon.org/methods/instance/directory/
use actix_web::{get, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{get_database_client, DbPool};
use crate::errors::HttpError;
use crate::mastodon_api::{
    accounts::types::Account,
    oauth::auth::get_current_user,
};
use crate::models::profiles::queries::get_profiles;
use super::types::DirectoryQueryParams;

#[get("")]
async fn profile_directory(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<DirectoryQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    get_current_user(db_client, auth.token()).await?;
    let profiles = get_profiles(
        db_client,
        query_params.local,
        query_params.offset,
        query_params.limit.inner(),
    ).await?;
    let accounts: Vec<Account> = profiles
        .into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

pub fn directory_api_scope() -> Scope {
    web::scope("/api/v1/directory")
        .service(profile_directory)
}
