/// https://docs.joinmastodon.org/methods/timelines/
use actix_web::{get, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::mastodon_api::statuses::helpers::build_status_list;
use crate::models::posts::queries::{
    get_home_timeline,
    get_local_timeline,
    get_posts_by_tag,
};
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
    let posts = get_home_timeline(
        db_client,
        &current_user.id,
        query_params.max_id,
        query_params.limit,
    ).await?;
    let statuses = build_status_list(
        db_client,
        &config.instance_url(),
        Some(&current_user),
        posts,
    ).await?;
    Ok(HttpResponse::Ok().json(statuses))
}

/// Local timeline ("local" parameter is ignored)
#[get("/public")]
async fn public_timeline(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let posts = get_local_timeline(
        db_client,
        &current_user.id,
        query_params.max_id,
        query_params.limit,
    ).await?;
    let statuses = build_status_list(
        db_client,
        &config.instance_url(),
        Some(&current_user),
        posts,
    ).await?;
    Ok(HttpResponse::Ok().json(statuses))
}

#[get("/tag/{hashtag}")]
async fn hashtag_timeline(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    hashtag: web::Path<String>,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let posts = get_posts_by_tag(
        db_client,
        &hashtag,
        maybe_current_user.as_ref().map(|user| &user.id),
        query_params.max_id,
        query_params.limit,
    ).await?;
    let statuses = build_status_list(
        db_client,
        &config.instance_url(),
        maybe_current_user.as_ref(),
        posts,
    ).await?;
    Ok(HttpResponse::Ok().json(statuses))
}

pub fn timeline_api_scope() -> Scope {
    web::scope("/api/v1/timelines")
        .service(home_timeline)
        .service(public_timeline)
        .service(hashtag_timeline)
}
