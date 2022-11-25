use actix_web::{get, post, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::{
    accounts::types::Account,
    oauth::auth::get_current_user,
};
use crate::models::users::queries::set_user_password;
use crate::utils::passwords::hash_password;
use super::helpers::{export_followers, export_follows};
use super::types::PasswordChangeRequest;

#[post("/change_password")]
async fn change_password_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    request_data: web::Json<PasswordChangeRequest>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let password_hash = hash_password(&request_data.new_password)
        .map_err(|_| HttpError::InternalError)?;
    set_user_password(db_client, &current_user.id, password_hash).await?;
    let account = Account::from_user(current_user, &config.instance_url());
    Ok(HttpResponse::Ok().json(account))
}

#[get("/export_followers")]
async fn export_followers_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let csv = export_followers(
        db_client,
        &config.instance().hostname(),
        &current_user.id,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type("text/csv")
        .body(csv);
    Ok(response)
}

#[get("/export_follows")]
async fn export_follows_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let csv = export_follows(
        db_client,
        &config.instance().hostname(),
        &current_user.id,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type("text/csv")
        .body(csv);
    Ok(response)
}

pub fn settings_api_scope() -> Scope {
    web::scope("/api/v1/settings")
        .service(change_password_view)
        .service(export_followers_view)
        .service(export_follows_view)
}
