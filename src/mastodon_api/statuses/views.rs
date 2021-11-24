/// https://docs.joinmastodon.org/methods/statuses/
use actix_web::{get, post, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use serde::Serialize;
use uuid::Uuid;

use crate::activitypub::activity::{
    create_activity_note,
    create_activity_like,
};
use crate::activitypub::actor::Actor;
use crate::activitypub::deliverer::deliver_activity;
use crate::activitypub::views::get_object_url;
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{DatabaseError, HttpError};
use crate::ethereum::nft::create_mint_signature;
use crate::ipfs::store as ipfs_store;
use crate::ipfs::utils::{IPFS_LOGO, get_ipfs_url};
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::models::attachments::queries::set_attachment_ipfs_cid;
use crate::models::posts::helpers::can_view_post;
use crate::models::posts::mentions::{find_mentioned_profiles, replace_mentions};
use crate::models::profiles::queries::get_followers;
use crate::models::posts::helpers::{
    get_actions_for_posts,
    get_reposted_posts,
};
use crate::models::posts::queries::{
    create_post,
    get_post_by_id,
    get_thread,
    find_reposts_by_user,
    update_post,
    delete_post,
};
use crate::models::posts::types::PostCreateData;
use crate::models::reactions::queries::{
    create_reaction,
    delete_reaction,
};
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
    let instance = config.instance();
    let mut post_data = PostCreateData::from(data.into_inner());
    post_data.validate()?;
    // Mentions
    let mention_map = find_mentioned_profiles(
        db_client,
        &instance.host(),
        &post_data.content,
    ).await?;
    post_data.content = replace_mentions(
        &mention_map,
        &instance.host(),
        &instance.url(),
        &post_data.content,
    );
    post_data.mentions = mention_map.values()
        .map(|profile| profile.id).collect();
    let post = create_post(db_client, &current_user.id, post_data).await?;
    // Federate
    let maybe_in_reply_to = match post.in_reply_to_id {
        Some(in_reply_to_id) => {
            let in_reply_to = get_post_by_id(db_client, &in_reply_to_id).await?;
            Some(in_reply_to)
        },
        None => None,
    };
    let activity = create_activity_note(
        &instance.host(),
        &instance.url(),
        &post,
        maybe_in_reply_to.as_ref(),
    );
    let followers = get_followers(db_client, &current_user.id).await?;
    let mut recipients: Vec<Actor> = Vec::new();
    for follower in followers {
        let maybe_remote_actor = follower.remote_actor()
            .map_err(|_| HttpError::InternalError)?;
        if let Some(remote_actor) = maybe_remote_actor {
            recipients.push(remote_actor);
        };
    };
    if let Some(in_reply_to) = maybe_in_reply_to {
        let maybe_remote_actor = in_reply_to.author.remote_actor()
            .map_err(|_| HttpError::InternalError)?;
        if let Some(remote_actor) = maybe_remote_actor {
            recipients.push(remote_actor);
        }
    }
    for profile in post.mentions.iter() {
        let maybe_remote_actor = profile.remote_actor()
            .map_err(|_| HttpError::InternalError)?;
        if let Some(remote_actor) = maybe_remote_actor {
            recipients.push(remote_actor);
        };
    };
    deliver_activity(&config, &current_user, activity, recipients);
    let status = Status::from_post(post, &instance.url());
    Ok(HttpResponse::Created().json(status))
}

#[get("/{status_id}")]
async fn get_status(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if !can_view_post(maybe_current_user.as_ref(), &post) {
        return Err(HttpError::NotFoundError("post"));
    };
    get_reposted_posts(db_client, vec![&mut post]).await?;
    if let Some(user) = maybe_current_user {
        get_actions_for_posts(db_client, &user.id, vec![&mut post]).await?;
    }
    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
}

#[get("/{status_id}/context")]
async fn get_context(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let mut posts = get_thread(
        db_client,
        &status_id,
        maybe_current_user.as_ref().map(|user| &user.id),
    ).await?;
    get_reposted_posts(db_client, posts.iter_mut().collect()).await?;
    if let Some(user) = maybe_current_user {
        get_actions_for_posts(
            db_client,
            &user.id,
            posts.iter_mut().collect(),
        ).await?;
    }
    let statuses: Vec<Status> = posts
        .into_iter()
        .map(|post| Status::from_post(post, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(statuses))
}

#[post("/{status_id}/favourite")]
async fn favourite(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id(db_client, &status_id).await?;
    if !can_view_post(Some(&current_user), &post) {
        return Err(HttpError::NotFoundError("post"));
    };
    let reaction_created = match create_reaction(
        db_client, &current_user.id, &status_id,
    ).await {
        Ok(_) => true,
        Err(DatabaseError::AlreadyExists(_)) => false, // post already favourited
        Err(other_error) => return Err(other_error.into()),
    };
    let mut post = get_post_by_id(db_client, &status_id).await?;
    get_reposted_posts(db_client, vec![&mut post]).await?;
    get_actions_for_posts(db_client, &current_user.id, vec![&mut post]).await?;

    if reaction_created {
        let maybe_remote_actor = post.author.remote_actor()
            .map_err(|_| HttpError::InternalError)?;
        if let Some(remote_actor) = maybe_remote_actor {
            // Federate
            let object_id = post.object_id.as_ref().ok_or(HttpError::InternalError)?;
            let activity = create_activity_like(
                &config.instance_url(),
                &current_user.profile,
                object_id,
            );
            deliver_activity(&config, &current_user, activity, vec![remote_actor]);
        }
    }

    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
}

#[post("/{status_id}/unfavourite")]
async fn unfavourite(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id(db_client, &status_id).await?;
    if !can_view_post(Some(&current_user), &post) {
        return Err(HttpError::NotFoundError("post"));
    };
    match delete_reaction(db_client, &current_user.id, &status_id).await {
        Err(DatabaseError::NotFound(_)) => (), // post not favourited
        other_result => other_result?,
    }
    let mut post = get_post_by_id(db_client, &status_id).await?;
    get_reposted_posts(db_client, vec![&mut post]).await?;
    get_actions_for_posts(db_client, &current_user.id, vec![&mut post]).await?;
    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
}

#[post("/{status_id}/reblog")]
async fn reblog(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let repost_data = PostCreateData {
        repost_of_id: Some(status_id),
        ..Default::default()
    };
    create_post(db_client, &current_user.id, repost_data).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    get_reposted_posts(db_client, vec![&mut post]).await?;
    get_actions_for_posts(db_client, &current_user.id, vec![&mut post]).await?;
    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
}

#[post("/{status_id}/unreblog")]
async fn unreblog(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let reposts = find_reposts_by_user(db_client, &current_user.id, &[status_id]).await?;
    let repost_id = reposts.first().ok_or(HttpError::NotFoundError("post"))?;
    // Ignore returned data because reposts don't have attached files
    delete_post(db_client, repost_id).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    get_reposted_posts(db_client, vec![&mut post]).await?;
    get_actions_for_posts(db_client, &current_user.id, vec![&mut post]).await?;
    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
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
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if post.author.id != current_user.id || !post.is_public() {
        // Users can only archive their own public posts
        return Err(HttpError::NotFoundError("post"));
    };
    let ipfs_api_url = config.ipfs_api_url.as_ref()
        .ok_or(HttpError::NotSupported)?;

    let post_image_cid = if let Some(attachment) = post.attachments.first() {
        // Add attachment to IPFS
        let image_path = config.media_dir().join(&attachment.file_name);
        let image_data = std::fs::read(image_path)
            .map_err(|_| HttpError::InternalError)?;
        let image_cid = ipfs_store::add(ipfs_api_url, image_data).await
            .map_err(|_| HttpError::InternalError)?;
        set_attachment_ipfs_cid(db_client, &attachment.id, &image_cid).await?;
        image_cid
    } else {
        // Use IPFS logo if there's no image
        IPFS_LOGO.to_string()
    };
    let post_url = get_object_url(
        &config.instance_url(),
        &post.id,
    );
    let post_metadata = PostMetadata {
        name: format!("Post {}", post.id),
        description: post.content.clone(),
        image: get_ipfs_url(&post_image_cid),
        external_url: post_url,
    };
    let post_metadata_json = serde_json::to_string(&post_metadata)
        .map_err(|_| HttpError::InternalError)?
        .as_bytes().to_vec();
    let post_metadata_cid = ipfs_store::add(ipfs_api_url, post_metadata_json).await
        .map_err(|_| HttpError::InternalError)?;

    // Update post
    post.ipfs_cid = Some(post_metadata_cid);
    update_post(db_client, &post).await?;
    get_reposted_posts(db_client, vec![&mut post]).await?;
    get_actions_for_posts(db_client, &current_user.id, vec![&mut post]).await?;
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
    if post.author.id != current_user.id || !post.is_public() {
        // Users can only tokenize their own public posts
        return Err(HttpError::NotFoundError("post"));
    };
    let ipfs_cid = post.ipfs_cid
        // Post metadata is not immutable
        .ok_or(HttpError::ValidationError("post is not immutable".into()))?;
    let token_uri = get_ipfs_url(&ipfs_cid);
    let signature = create_mint_signature(
        contract_config,
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
        .service(favourite)
        .service(unfavourite)
        .service(reblog)
        .service(unreblog)
        .service(make_permanent)
        .service(get_signature)
}
