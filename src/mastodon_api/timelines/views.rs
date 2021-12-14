/// https://docs.joinmastodon.org/methods/timelines/
use actix_web::{get, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::mastodon_api::statuses::types::Status;
use crate::models::posts::helpers::{
    get_actions_for_posts,
    get_reposted_posts,
};
use crate::models::posts::queries::{get_home_timeline, get_posts_by_tag};
use super::types::TimelineQueryParams;

#[get("/home")]
async fn home_timeline(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut posts = get_home_timeline(
        db_client,
        &current_user.id,
        query_params.max_id,
        query_params.limit,
    ).await?;
    get_reposted_posts(db_client, posts.iter_mut().collect()).await?;
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

#[get("/tag/{hashtag}")]
async fn hashtag_timeline(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(hashtag): web::Path<String>,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let mut posts = get_posts_by_tag(
        db_client,
        &hashtag,
        query_params.max_id,
        query_params.limit,
    ).await?;
    get_reposted_posts(db_client, posts.iter_mut().collect()).await?;
    if let Some(user) = maybe_current_user {
        get_actions_for_posts(
            db_client,
            &user.id,
            posts.iter_mut().collect(),
        ).await?;
    };
    let statuses: Vec<Status> = posts
        .into_iter()
        .map(|post| Status::from_post(post, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(statuses))
}

pub fn timeline_api_scope() -> Scope {
    web::scope("/api/v1/timelines")
        .service(home_timeline)
        .service(hashtag_timeline)
}
