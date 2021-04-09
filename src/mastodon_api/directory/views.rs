use actix_session::Session;
use actix_web::{get, web, HttpResponse};

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::users::auth::get_current_user;
use crate::models::profiles::queries::get_profiles;

#[get("/api/v1/directory")]
pub async fn profile_directory(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    session: Session,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    get_current_user(db_client, session).await?;
    let accounts: Vec<Account> = get_profiles(db_client).await?
        .into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}
