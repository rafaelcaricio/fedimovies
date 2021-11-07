use actix_web::{get, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::models::profiles::queries::get_profiles;

#[get("")]
async fn profile_directory(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    get_current_user(db_client, auth.token()).await?;
    let accounts: Vec<Account> = get_profiles(db_client).await?
        .into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

pub fn directory_api_scope() -> Scope {
    web::scope("/api/v1/directory")
        .service(profile_directory)
}
