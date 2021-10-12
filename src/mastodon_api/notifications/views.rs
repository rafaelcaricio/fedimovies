/// https://docs.joinmastodon.org/methods/notifications/
use actix_web::{get, web, HttpResponse, Scope as ActixScope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::models::notifications::queries::get_notifications;
use super::types::ApiNotification;

#[get("")]
async fn get_notifications_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let notifications: Vec<ApiNotification> = get_notifications(
        db_client,
        &current_user.id,
    ).await?
        .into_iter()
        .map(|item| ApiNotification::from_db(item, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(notifications))
}

pub fn notification_api_scope() -> ActixScope {
    web::scope("/api/v1/notifications")
        .service(get_notifications_view)
}
