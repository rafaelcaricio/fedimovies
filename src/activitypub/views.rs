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
use crate::database::{get_database_client, DbPool};
use crate::errors::HttpError;
use crate::models::{
    emojis::queries::get_local_emoji_by_name,
    posts::helpers::{add_related_posts, can_view_post},
    posts::queries::{get_post_by_id, get_posts_by_author},
    users::queries::get_user_by_name,
};
use crate::web_client::urls::{
    get_post_page_url,
    get_profile_page_url,
    get_tag_page_url,
};
use super::actors::types::{get_local_actor, get_instance_actor};
use super::builders::create_note::{
    build_emoji_tag,
    build_note,
    build_create_note,
};
use super::collections::{
    COLLECTION_PAGE_SIZE,
    OrderedCollection,
    OrderedCollectionPage,
};
use super::constants::{AP_MEDIA_TYPE, AS_MEDIA_TYPE};
use super::identifiers::{
    local_actor_followers,
    local_actor_following,
    local_actor_subscribers,
    local_actor_outbox,
};
use super::receiver::receive_activity;

pub fn is_activitypub_request(headers: &HeaderMap) -> bool {
    let maybe_user_agent = headers.get("User-Agent")
        .and_then(|value| value.to_str().ok());
    if let Some(user_agent) = maybe_user_agent {
        if user_agent.contains("THIS. IS. GNU social!!!!") {
            // GNU Social doesn't send valid Accept headers
            return true;
        };
    };
    const CONTENT_TYPES: [&str; 4] = [
        AP_MEDIA_TYPE,
        AS_MEDIA_TYPE,
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
    db_pool: web::Data<DbPool>,
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
        .content_type(AP_MEDIA_TYPE)
        .json(actor);
    Ok(response)
}

#[post("/inbox")]
async fn inbox(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    inbox_mutex: web::Data<Mutex<()>>,
    request: HttpRequest,
    activity: web::Json<serde_json::Value>,
) -> Result<HttpResponse, HttpError> {
    log::debug!("received activity: {}", activity);
    let activity_type = activity["type"].as_str().unwrap_or("Unknown");
    log::info!("received in {}: {}", request.uri().path(), activity_type);
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
        .map_err(|error| {
            log::warn!(
                "failed to process activity ({}): {}",
                error,
                activity,
            );
            error
        })?;
    Ok(HttpResponse::Accepted().finish())
}

#[derive(Deserialize)]
struct CollectionQueryParams {
    page: Option<bool>,
}

#[get("/outbox")]
async fn outbox(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let instance = config.instance();
    let collection_id = local_actor_outbox(&instance.url(), &username);
    let first_page_id = format!("{}?page=true", collection_id);
    if query_params.page.is_none() {
        let collection = OrderedCollection::new(
            collection_id,
            Some(first_page_id),
            None,
        );
        let response = HttpResponse::Ok()
            .content_type(AP_MEDIA_TYPE)
            .json(collection);
        return Ok(response);
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    // Posts are ordered by creation date
    let mut posts = get_posts_by_author(
        db_client,
        &user.id,
        None, // include only public posts
        false, // exclude replies
        false, // exclude reposts
        None,
        COLLECTION_PAGE_SIZE,
    ).await?;
    add_related_posts(db_client, posts.iter_mut().collect()).await?;
    let activities: Vec<_> = posts.iter().filter_map(|post| {
        if post.in_reply_to_id.is_some() || post.repost_of_id.is_some() {
            return None;
        };
        let activity = build_create_note(
            &instance.hostname(),
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
        .content_type(AP_MEDIA_TYPE)
        .json(collection_page);
    Ok(response)
}

#[post("/outbox")]
async fn outbox_client_to_server() -> HttpResponse {
    HttpResponse::MethodNotAllowed().finish()
}

#[get("/followers")]
async fn followers_collection(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    if query_params.page.is_some() {
        // Social graph is not available
        return Err(HttpError::PermissionError);
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    let collection_id = local_actor_followers(
        &config.instance_url(),
        &username,
    );
    let collection = OrderedCollection::new(
        collection_id,
        None,
        Some(user.profile.follower_count),
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection);
    Ok(response)
}

#[get("/following")]
async fn following_collection(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    if query_params.page.is_some() {
        // Social graph is not available
        return Err(HttpError::PermissionError);
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    let collection_id = local_actor_following(
        &config.instance_url(),
        &username,
    );
    let collection = OrderedCollection::new(
        collection_id,
        None,
        Some(user.profile.following_count),
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection);
    Ok(response)
}

#[get("/subscribers")]
async fn subscribers_collection(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    username: web::Path<String>,
    query_params: web::Query<CollectionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    if query_params.page.is_some() {
        // Subscriber list is hidden
        return Err(HttpError::PermissionError);
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    let collection_id = local_actor_subscribers(
        &config.instance_url(),
        &username,
    );
    let collection = OrderedCollection::new(
        collection_id,
        None,
        Some(user.profile.subscriber_count),
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(collection);
    Ok(response)
}

pub fn actor_scope() -> Scope {
    web::scope("/users/{username}")
        .service(actor_view)
        .service(inbox)
        .service(outbox)
        .service(outbox_client_to_server)
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
        .content_type(AP_MEDIA_TYPE)
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
    Ok(HttpResponse::Accepted().finish())
}

pub fn instance_actor_scope() -> Scope {
    web::scope("/actor")
        .service(instance_actor_view)
        .service(instance_actor_inbox)
}

#[get("/objects/{object_id}")]
pub async fn object_view(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    request: HttpRequest,
    internal_object_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let internal_object_id = internal_object_id.into_inner();
    // Try to find local post by ID,
    // return 404 if not found, or not public, or it is a repost
    let mut post = get_post_by_id(db_client, &internal_object_id).await?;
    if !post.is_local() || !can_view_post(db_client, None, &post).await? {
        return Err(HttpError::NotFoundError("post"));
    };
    if !is_activitypub_request(request.headers()) {
        let page_url = get_post_page_url(&config.instance_url(), &post.id);
        let response = HttpResponse::Found()
            .append_header(("Location", page_url))
            .finish();
        return Ok(response);
    };
    add_related_posts(db_client, vec![&mut post]).await?;
    let object = build_note(
        &config.instance().hostname(),
        &config.instance().url(),
        &post,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(object);
    Ok(response)
}

#[get("/objects/emojis/{emoji_name}")]
pub async fn emoji_view(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    emoji_name: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let emoji = get_local_emoji_by_name(
        db_client,
        &emoji_name,
    ).await?;
    let object = build_emoji_tag(
        &config.instance().url(),
        &emoji,
    );
    let response = HttpResponse::Ok()
        .content_type(AP_MEDIA_TYPE)
        .json(object);
    Ok(response)
}

#[get("/collections/tags/{tag_name}")]
pub async fn tag_view(
    config: web::Data<Config>,
    tag_name: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let page_url = get_tag_page_url(&config.instance_url(), &tag_name);
    let response = HttpResponse::Found()
        .append_header(("Location", page_url))
        .finish();
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
