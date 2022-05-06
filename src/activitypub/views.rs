use std::time::Instant;

use actix_web::{
    get, post, web,
    HttpRequest, HttpResponse, Scope,
    http::header::HeaderMap,
};
use serde::Deserialize;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::frontend::{get_post_page_url, get_profile_page_url};
use crate::models::posts::queries::{get_posts_by_author, get_thread};
use crate::models::users::queries::get_user_by_name;
use super::activity::{create_note, create_activity_note};
use super::actor::{get_local_actor, get_instance_actor};
use super::collections::{
    COLLECTION_PAGE_SIZE,
    OrderedCollection,
    OrderedCollectionPage,
};
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

pub fn get_subscribers_url(instance_url: &str, username: &str) -> String {
    format!("{}/users/{}/subscribers", instance_url, username)
}

pub fn get_instance_actor_url(instance_url: &str) -> String {
    format!("{}/actor", instance_url)
}

pub fn get_object_url(instance_url: &str, internal_object_id: &Uuid) -> String {
    format!("{}/objects/{}", instance_url, internal_object_id)
}

fn is_activitypub_request(headers: &HeaderMap) -> bool {
    const CONTENT_TYPES: [&str; 4] = [
        ACTIVITY_CONTENT_TYPE,
        "application/activity+json",
        "application/ld+json",
        "application/json",
    ];
    if let Some(content_type) = headers.get("Accept") {
        let content_type_str = content_type.to_str().ok()
            // Take first content type if there are many
            .and_then(|value| value.split(',').next())
            .unwrap_or("");
        return CONTENT_TYPES.contains(&content_type_str);
    };
    false
}

#[get("")]
async fn actor_view(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    request: HttpRequest,
    username: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    if !is_activitypub_request(request.headers()) {
        let page_url = get_profile_page_url(&config.instance_url(), &user.id);
        let response = HttpResponse::Found()
            .append_header(("Location", page_url))
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
    inbox_mutex: web::Data<Mutex<()>>,
    request: HttpRequest,
    activity: web::Json<serde_json::Value>,
) -> Result<HttpResponse, HttpError> {
    log::debug!("received activity: {}", activity);
    let activity_type = activity["type"].as_str().unwrap_or("Unknown");
    if activity_type == DELETE && activity["actor"] == activity["object"] {
        // Ignore Delete(Person) activities and skip signature verification
        log::info!("received in {}: Delete(Person)", request.uri().path());
        return Ok(HttpResponse::Ok().finish());
    } else {
        log::info!("received in {}: {}", request.uri().path(), activity_type);
    };
    let now = Instant::now();
    // Store mutex guard in a variable to prevent it from being dropped immediately
    let _guard = inbox_mutex.lock().await;
    log::debug!(
        "acquired inbox lock after waiting for {:.2?}: {}",
        now.elapsed(),
        activity["id"].as_str().unwrap_or_default(),
    );
    let db_client = &mut **get_database_client(&db_pool).await?;
    receive_activity(&config, db_client, &request, &activity).await
        .map_err(|err| {
            log::warn!("failed to process activity ({}): {}", err, activity);
            err
        })?;
    Ok(HttpResponse::Ok().finish())
}

#[derive(Deserialize)]
struct CollectionQueryParams {
    page: Option<bool>,
}

#[get("/outbox")]
async fn outbox(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let instance = config.instance();
    let collection_id = get_outbox_url(&instance.url(), &username);
    let first_page_id = format!("{}?page=true", collection_id);
    if query_params.page.is_none() {
        let collection = OrderedCollection::new(
            collection_id,
            Some(first_page_id),
        );
        let response = HttpResponse::Ok()
            .content_type(ACTIVITY_CONTENT_TYPE)
            .json(collection);
        return Ok(response);
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    // Posts are ordered by creation date
    let posts = get_posts_by_author(
        db_client,
        &user.id,
        None, // include only public posts
        false, // exclude replies
        false, // exclude reposts
        None, COLLECTION_PAGE_SIZE,
    ).await?;
    let activities: Vec<_> = posts.iter().filter_map(|post| {
        if post.in_reply_to_id.is_some() || post.repost_of_id.is_some() {
            return None;
        };
        // Replies are not included so post.in_reply_to
        // does not need to be populated
        let activity = create_activity_note(
            &instance.host(),
            &instance.url(),
            post,
        );
        Some(activity)
    }).collect();
    let collection_page = OrderedCollectionPage::new(
        first_page_id,
        activities,
    );
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(collection_page);
    Ok(response)
}

#[get("/followers")]
async fn followers_collection(
    config: web::Data<Config>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    if query_params.page.is_some() {
        // Social graph is not available
        return Err(HttpError::PermissionError);
    }
    let collection_id = get_followers_url(&config.instance_url(), &username);
    let collection = OrderedCollection::new(collection_id, None);
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(collection);
    Ok(response)
}

#[get("/following")]
async fn following_collection(
    config: web::Data<Config>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    if query_params.page.is_some() {
        // Social graph is not available
        return Err(HttpError::PermissionError);
    }
    let collection_id = get_following_url(&config.instance_url(), &username);
    let collection = OrderedCollection::new(collection_id, None);
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(collection);
    Ok(response)
}

#[get("/subscribers")]
async fn subscribers_collection(
    config: web::Data<Config>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    if query_params.page.is_some() {
        // Subscriber list is hidden
        return Err(HttpError::PermissionError);
    }
    let collection_id = get_subscribers_url(&config.instance_url(), &username);
    let collection = OrderedCollection::new(collection_id, None);
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(collection);
    Ok(response)
}

pub fn actor_scope() -> Scope {
    web::scope("/users/{username}")
        .service(actor_view)
        .service(inbox)
        .service(outbox)
        .service(followers_collection)
        .service(following_collection)
        .service(subscribers_collection)
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
    internal_object_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    // Try to find local post by ID,
    // return 404 if not found or if repost is found
    let internal_object_id = internal_object_id.into_inner();
    let thread = get_thread(db_client, &internal_object_id, None).await?;
    let mut post = thread.iter()
        .find(|post| post.id == internal_object_id && post.author.is_local())
        .ok_or(HttpError::NotFoundError("post"))?
        .clone();
    if !is_activitypub_request(request.headers()) {
        let page_url = get_post_page_url(&config.instance_url(), &post.id);
        let response = HttpResponse::Found()
            .append_header(("Location", page_url))
            .finish();
        return Ok(response);
    };
    post.in_reply_to = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            let in_reply_to = thread.iter()
                .find(|post| post.id == in_reply_to_id)
                // Parent post must be present in thread
                .ok_or(HttpError::InternalError)?
                .clone();
            Some(Box::new(in_reply_to))
        },
        None => None,
    };
    let object = create_note(
        &config.instance().host(),
        &config.instance().url(),
        &post,
    );
    let response = HttpResponse::Ok()
        .content_type(ACTIVITY_CONTENT_TYPE)
        .json(object);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use actix_web::http::{
        header,
        header::{HeaderMap, HeaderValue},
    };
    use super::*;

    #[test]
    fn test_is_activitypub_request_mastodon() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::ACCEPT,
            HeaderValue::from_static(r#"application/activity+json, application/ld+json; profile="https://www.w3.org/ns/activitystreams", text/html;q=0.1"#),
        );
        let result = is_activitypub_request(&request_headers);
        assert_eq!(result, true);
    }

    #[test]
    fn test_is_activitypub_request_pleroma() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/activity+json"),
        );
        let result = is_activitypub_request(&request_headers);
        assert_eq!(result, true);
    }

    #[test]
    fn test_is_activitypub_request_browser() {
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("text/html"),
        );
        let result = is_activitypub_request(&request_headers);
        assert_eq!(result, false);
    }
}
