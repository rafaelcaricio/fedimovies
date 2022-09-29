/// https://docs.joinmastodon.org/methods/notifications/
use actix_web::{
    get, web,
    HttpRequest, HttpResponse,
    Scope as ActixScope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::mastodon_api::pagination::get_paginated_response;
use crate::models::notifications::queries::get_notifications;
use super::types::{ApiNotification, NotificationQueryParams};

#[get("")]
async fn get_notifications_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    query_params: web::Query<NotificationQueryParams>,
    request: HttpRequest,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let notifications: Vec<ApiNotification> = get_notifications(
        db_client,
        &current_user.id,
        query_params.max_id,
        query_params.limit,
    ).await?
        .into_iter()
        .map(|item| ApiNotification::from_db(item, &config.instance_url()))
        .collect();
    let max_index = usize::from(query_params.limit.saturating_sub(1));
    let maybe_last_id = notifications.get(max_index)
        .map(|item| item.id.clone());
    let response = get_paginated_response(
        &config.instance_url(),
        request.uri().path(),
        notifications,
        maybe_last_id,
    );
    Ok(response)
}

pub fn notification_api_scope() -> ActixScope {
    web::scope("/api/v1/notifications")
        .service(get_notifications_view)
}
