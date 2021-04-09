use actix_session::Session;
use actix_web::{get, web, HttpResponse};

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::statuses::types::Status;
use crate::mastodon_api::users::auth::get_current_user;
use crate::models::posts::queries::get_posts;

#[get("/api/v1/timelines/home")]
pub async fn home_timeline(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    session: Session,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, session).await?;
    let statuses: Vec<Status> = get_posts(db_client, &current_user.id).await?
        .into_iter()
        .map(|post| Status::from_post(post, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(statuses))
}
