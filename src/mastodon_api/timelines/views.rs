use actix_web::{get, web, HttpResponse};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::mastodon_api::statuses::types::Status;
use crate::models::posts::helpers::get_actions_for_posts;
use crate::models::posts::queries::get_posts;

/// https://docs.joinmastodon.org/methods/timelines/
#[get("/api/v1/timelines/home")]
pub async fn home_timeline(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut posts = get_posts(db_client, &current_user.id).await?;
    get_actions_for_posts(
        db_client,
        &current_user.id,
        posts.iter_mut().collect(),
    ).await?;
    let statuses: Vec<Status> = posts
        .into_iter()
        .map(|post| Status::from_post(post, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(statuses))
}
