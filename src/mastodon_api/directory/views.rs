/// https://docs.joinmastodon.org/methods/instance/directory/
use actix_web::{dev::ConnectionInfo, get, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DbPool},
    profiles::queries::get_profiles,
};

use super::types::DirectoryQueryParams;
use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::types::Account, errors::MastodonError, oauth::auth::get_current_user,
};

#[get("")]
async fn profile_directory(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<DirectoryQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    get_current_user(db_client, auth.token()).await?;
    let profiles = get_profiles(
        db_client,
        query_params.local,
        query_params.offset,
        query_params.limit.inner(),
    )
    .await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = profiles
        .into_iter()
        .map(|profile| Account::from_profile(&base_url, &instance_url, profile))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

pub fn directory_api_scope() -> Scope {
    web::scope("/api/v1/directory").service(profile_directory)
}
