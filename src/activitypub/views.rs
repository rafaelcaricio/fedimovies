use actix_web::{
    get, post, web,
    HttpRequest, HttpResponse, Scope,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::http_signatures::verify::verify_http_signature;
use crate::models::users::queries::get_user_by_name;
use super::activity::OrderedCollection;
use super::actor::get_actor_object;
use super::constants::ACTIVITY_CONTENT_TYPE;
use super::receiver::receive_activity;

pub fn get_actor_url(instance_url: &str, username: &str) -> String {
    format!("{}/users/{}", instance_url, username)
}

pub fn get_inbox_url(instance_url: &str, username: &str) -> String {
    format!("{}/users/{}/inbox", instance_url, username)
}

pub fn get_outbox_url(instance_url: &str, username: &str) -> String {
    format!("{}/users/{}/outbox", instance_url, username)
}

pub fn get_followers_url(instance_url: &str, username: &str) -> String {
    format!("{}/users/{}/followers", instance_url, username)
}

pub fn get_following_url(instance_url: &str, username: &str) -> String {
    format!("{}/users/{}/following", instance_url, username)
}

pub fn get_object_url(instance_url: &str, object_uuid: &Uuid) -> String {
    format!("{}/objects/{}", instance_url, object_uuid)
}

#[get("")]
async fn get_actor(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(username): web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    let actor = get_actor_object(&config, &user)?;
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(actor);
    Ok(response)
}

#[post("/inbox")]
async fn inbox(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    request: HttpRequest,
    web::Path(username): web::Path<String>,
    activity: web::Json<serde_json::Value>,
) -> Result<HttpResponse, HttpError> {
    log::info!("received to '{}' inbox: {}", username, activity);
    if let Err(err) = verify_http_signature(&config, &db_pool, &request).await {
        log::warn!("invalid signature: {}", err);
    }
    receive_activity(&config, &db_pool, username, activity.into_inner()).await?;
    Ok(HttpResponse::Ok().body("success"))
}

#[derive(Deserialize)]
struct CollectionQueryParams {
    page: Option<i32>,
}

#[get("/followers")]
async fn followers_collection(
    config: web::Data<Config>,
    web::Path(username): web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    if query_params.page.is_some() {
        // Social graph is not available
        return Err(HttpError::PermissionError);
    }
    let collection_url = get_followers_url(&config.instance_url(), &username);
    let collection = OrderedCollection::new(collection_url);
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(collection);
    Ok(response)
}

#[get("/following")]
async fn following_collection(
    config: web::Data<Config>,
    web::Path(username): web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    if query_params.page.is_some() {
        // Social graph is not available
        return Err(HttpError::PermissionError);
    }
    let collection_url = get_following_url(&config.instance_url(), &username);
    let collection = OrderedCollection::new(collection_url);
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(collection);
    Ok(response)
}

pub fn activitypub_scope() -> Scope {
    web::scope("/users/{username}")
        .service(get_actor)
        .service(inbox)
        .service(followers_collection)
        .service(following_collection)
}

#[get("/objects/{object_id}")]
pub async fn get_object(
    web::Path(_object_id): web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    // WARNING: activities/objects are not stored
    let response = HttpResponse::Gone().body("");
    Ok(response)
}
