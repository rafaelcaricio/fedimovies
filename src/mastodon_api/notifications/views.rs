/// https://docs.joinmastodon.org/methods/notifications/
use actix_web::{dev::ConnectionInfo, get, web, HttpRequest, HttpResponse, Scope as ActixScope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use fedimovies_config::Config;
use fedimovies_models::{
    database::{get_database_client, DbPool},
    notifications::queries::get_notifications,
};

use super::types::{ApiNotification, NotificationQueryParams};
use crate::http::get_request_base_url;
use crate::mastodon_api::{
    errors::MastodonError, oauth::auth::get_current_user, pagination::get_paginated_response,
};

#[get("")]
async fn get_notifications_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<NotificationQueryParams>,
    request: HttpRequest,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let base_url = get_request_base_url(connection_info);
    let instance = config.instance();
    let notifications: Vec<ApiNotification> = get_notifications(
        db_client,
        &current_user.id,
        query_params.max_id,
        query_params.limit.inner(),
    )
    .await?
    .into_iter()
    .map(|item| ApiNotification::from_db(&base_url, &instance.url(), item))
    .collect();
    let max_index = usize::from(query_params.limit.inner().saturating_sub(1));
    let maybe_last_id = notifications.get(max_index).map(|item| item.id.clone());
    let response = get_paginated_response(
        &instance.url(),
        request.uri().path(),
        notifications,
        maybe_last_id,
    );
    Ok(response)
}

pub fn notification_api_scope() -> ActixScope {
    web::scope("/api/v1/notifications").service(get_notifications_view)
}
