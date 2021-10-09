use actix_web::{get, post, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use serde::Serialize;
use uuid::Uuid;

use crate::activitypub::activity::create_activity_note;
use crate::activitypub::actor::Actor;
use crate::activitypub::deliverer::deliver_activity;
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::ethereum::nft::create_mint_signature;
use crate::ipfs::store as ipfs_store;
use crate::ipfs::utils::{IPFS_LOGO, get_ipfs_url};
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::models::attachments::queries::set_attachment_ipfs_cid;
use crate::models::profiles::queries::get_followers;
use crate::models::posts::queries::{
    create_post,
    get_post_by_id,
    get_thread,
    update_post,
};
use crate::models::posts::types::PostCreateData;
use super::types::{Status, StatusData};

#[post("")]
async fn create_status(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    data: web::Json<StatusData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post_data = PostCreateData::from(data.into_inner());
    post_data.validate()?;
    let post = create_post(db_client, &current_user.id, post_data).await?;
    // Federate
    let in_reply_to = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            let in_reply_to = get_post_by_id(db_client, &in_reply_to_id).await?;
            Some(in_reply_to)
        },
        None => None,
    };
    let activity = create_activity_note(
        &config.instance_url(),
        &post,
        in_reply_to.as_ref(),
    );
    let followers = get_followers(db_client, &current_user.id).await?;
    let mut recipients: Vec<Actor> = Vec::new();
    for follower in followers {
        if let Some(actor_value) = follower.actor_json {
            // Remote
            let actor: Actor = serde_json::from_value(actor_value)
                .map_err(|_| HttpError::InternalError)?;
            recipients.push(actor);
        };
    };
    let config_clone = config.clone();
    actix_rt::spawn(async move {
        deliver_activity(
            &config_clone,
            &current_user,
            activity,
            recipients,
        ).await;
    });
    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Created().json(status))
}

#[get("/{status_id}")]
async fn get_status(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let post = get_post_by_id(db_client, &status_id).await?;
    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
}

#[get("/{status_id}/context")]
async fn get_context(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let statuses: Vec<Status> = get_thread(db_client, &status_id).await?
        .into_iter()
        .map(|post| Status::from_post(post, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(statuses))
}

// https://docs.opensea.io/docs/metadata-standards
#[derive(Serialize)]
struct PostMetadata {
    name: String,
    description: String,
    image: String,
    external_url: String,
}

#[post("/{status_id}/make_permanent")]
async fn make_permanent(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    let ipfs_api_url = config.ipfs_api_url.as_ref()
        .ok_or(HttpError::NotSupported)?;

    let post_image_cid = if let Some(attachment) = post.attachments.first() {
        // Add attachment to IPFS
        let image_path = config.media_dir().join(&attachment.file_name);
        let image_data = std::fs::read(image_path)
            .map_err(|_| HttpError::InternalError)?;
        let image_cid = ipfs_store::add(&ipfs_api_url, image_data).await
            .map_err(|_| HttpError::InternalError)?;
        set_attachment_ipfs_cid(db_client, &attachment.id, &image_cid).await?;
        image_cid
    } else {
        // Use IPFS logo if there's no image
        IPFS_LOGO.to_string()
    };
    let post_metadata = PostMetadata {
        name: format!("Post {}", post.id),
        description: post.content.clone(),
        image: get_ipfs_url(&post_image_cid),
        // TODO: use absolute URL
        external_url: format!("/post/{}", post.id),
    };
    let post_metadata_json = serde_json::to_string(&post_metadata)
        .map_err(|_| HttpError::InternalError)?
        .as_bytes().to_vec();
    let post_metadata_cid = ipfs_store::add(&ipfs_api_url, post_metadata_json).await
        .map_err(|_| HttpError::InternalError)?;

    // Update post
    post.ipfs_cid = Some(post_metadata_cid);
    update_post(db_client, &post).await?;
    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
}

#[get("/{status_id}/signature")]
async fn get_signature(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let contract_config = config.ethereum_contract.as_ref()
        .ok_or(HttpError::NotSupported)?;
    let post = get_post_by_id(db_client, &status_id).await?;
    if post.author.id != current_user.id {
        // Users can only tokenize their own posts
        Err(HttpError::NotFoundError("post"))?;
    }
    let ipfs_cid = post.ipfs_cid
        // Post metadata is not immutable
        .ok_or(HttpError::ValidationError("post is not immutable".into()))?;
    let token_uri = get_ipfs_url(&ipfs_cid);
    let signature = create_mint_signature(
        &contract_config,
        &current_user.wallet_address,
        &token_uri,
    ).map_err(|_| HttpError::InternalError)?;
    Ok(HttpResponse::Ok().json(signature))
}

pub fn status_api_scope() -> Scope {
    web::scope("/api/v1/statuses")
        // Routes without status ID
        .service(create_status)
        // Routes with status ID
        .service(get_status)
        .service(get_context)
        .service(make_permanent)
        .service(get_signature)
}
