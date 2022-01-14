/// https://docs.joinmastodon.org/methods/notifications/
use actix_web::{get, web, HttpResponse, Scope as ActixScope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::models::notifications::queries::get_notifications;
use super::types::{ApiNotification, NotificationQueryParams};

fn get_pagination_header(
    instance_url: &str,
    last_id: &str,
) -> String {
    let next_page_url = format!(
        "{}/api/v1/notifications?max_id={}",
        instance_url,
        last_id
    );
    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Link
    format!(r#"<{}>; rel="next""#, next_page_url)
}

#[get("")]
async fn get_notifications_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    query_params: web::Query<NotificationQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let notifications: Vec<ApiNotification> = get_notifications(
        db_client,
        &current_user.id,
        query_params.max_id,
        query_params.limit.into(),
    ).await?
        .into_iter()
        .map(|item| ApiNotification::from_db(item, &config.instance_url()))
        .collect();
    let max_index = usize::from(query_params.limit - 1);
    let response = if let Some(item) = notifications.get(max_index) {
        let pagination_header = get_pagination_header(&config.instance_url(), &item.id);
        HttpResponse::Ok()
            .header("Link", pagination_header)
            // Link header needs to be exposed
            // https://github.com/actix/actix-extras/issues/192
            .header("Access-Control-Expose-Headers", "Link")
            .json(notifications)
    } else {
        HttpResponse::Ok().json(notifications)
    };
    Ok(response)
}

pub fn notification_api_scope() -> ActixScope {
    web::scope("/api/v1/notifications")
        .service(get_notifications_view)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE_URL: &str = "https://example.org";

    #[test]
    fn test_get_next_page_link() {
        let result = get_pagination_header(INSTANCE_URL, "123");
        assert_eq!(
            result,
            r#"<https://example.org/api/v1/notifications?max_id=123>; rel="next""#,
        );
    }
}
