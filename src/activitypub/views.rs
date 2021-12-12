use actix_web::{
    get, post, web,
    HttpRequest, HttpResponse, Scope,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::frontend::{get_post_page_url, get_profile_page_url};
use crate::http_signatures::verify::verify_http_signature;
use crate::models::posts::queries::get_thread;
use crate::models::users::queries::get_user_by_name;
use super::activity::{create_note, OrderedCollection};
use super::actor::{get_local_actor, get_instance_actor};
use super::constants::ACTIVITY_CONTENT_TYPE;
use super::receiver::receive_activity;
use super::vocabulary::DELETE;

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

pub fn get_instance_actor_url(instance_url: &str) -> String {
    format!("{}/actor", instance_url)
}

pub fn get_object_url(instance_url: &str, internal_object_id: &Uuid) -> String {
    format!("{}/objects/{}", instance_url, internal_object_id)
}

fn is_activitypub_request(request: &HttpRequest) -> bool {
    const CONTENT_TYPES: [&str; 5] = [
        ACTIVITY_CONTENT_TYPE,
        "application/activity+json, application/ld+json",  // Mastodon
        "application/activity+json",
        "application/ld+json",
        "application/json",
    ];
    if let Some(content_type) = request.headers().get("Accept") {
        let content_type_str = content_type.to_str().unwrap_or("");
        return CONTENT_TYPES.contains(&content_type_str);
    };
    false
}

#[get("")]
async fn actor_view(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    request: HttpRequest,
    web::Path(username): web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    if !is_activitypub_request(&request) {
        let page_url = get_profile_page_url(&config.instance_url(), &user.id);
        let response = HttpResponse::Found()
            .header("Location", page_url)
            .finish();
        return Ok(response);
    };
    let actor = get_local_actor(&user, &config.instance_url())
        .map_err(|_| HttpError::InternalError)?;
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
    activity: web::Json<serde_json::Value>,
) -> Result<HttpResponse, HttpError> {
    let signature_verified = verify_http_signature(&config, &db_pool, &request).await;
    if activity["type"].as_str() == Some(DELETE) && signature_verified.is_err() {
        // Don't log Delete() activities if HTTP signature is not valid
        log::info!("received in {}: Delete", request.uri().path());
    } else {
        log::info!("received in {}: {}", request.uri().path(), activity);
    };
    match signature_verified {
        Ok(signer_id) => log::info!("activity signed by {}", signer_id),
        Err(err) => log::warn!("invalid signature: {}", err),
    };
    receive_activity(&config, &db_pool, activity.into_inner()).await
        .map_err(|err| {
            log::info!("failed to process activity: {}", err);
            err
        })?;
    Ok(HttpResponse::Ok().finish())
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

pub fn actor_scope() -> Scope {
    web::scope("/users/{username}")
        .service(actor_view)
        .service(inbox)
        .service(followers_collection)
        .service(following_collection)
}

#[get("")]
async fn instance_actor_view(
    config: web::Data<Config>,
) -> Result<HttpResponse, HttpError> {
    let actor = get_instance_actor(&config.instance())
        .map_err(|_| HttpError::InternalError)?;
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(actor);
    Ok(response)
}

#[post("/inbox")]
async fn instance_actor_inbox(
    activity: web::Json<serde_json::Value>,
) -> Result<HttpResponse, HttpError> {
    log::info!(
        "received in instance inbox: {}",
        activity["type"].as_str().unwrap_or("Unknown"),
    );
    Ok(HttpResponse::Ok().finish())
}

pub fn instance_actor_scope() -> Scope {
    web::scope("/actor")
        .service(instance_actor_view)
        .service(instance_actor_inbox)
}

#[get("/objects/{object_id}")]
pub async fn object_view(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    request: HttpRequest,
    web::Path(internal_object_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    // Try to find local post by ID, return 404 if not found
    let thread = get_thread(db_client, &internal_object_id, None).await?;
    let post = thread.iter()
        .find(|post| post.id == internal_object_id && post.author.is_local())
        .ok_or(HttpError::NotFoundError("post"))?;
    if !is_activitypub_request(&request) {
        let page_url = get_post_page_url(&config.instance_url(), &post.id);
        let response = HttpResponse::Found()
            .header("Location", page_url)
            .finish();
        return Ok(response);
    };
    let in_reply_to = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            thread.iter().find(|post| post.id == in_reply_to_id)
        },
        None => None,
    };
    let object = create_note(
        &config.instance().host(),
        &config.instance().url(),
        post,
        in_reply_to,
    );
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(object);
    Ok(response)
}
