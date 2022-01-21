/// https://docs.joinmastodon.org/methods/statuses/
use std::convert::TryFrom;

use actix_web::{delete, get, post, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use crate::activitypub::activity::{
    create_activity_note,
    create_activity_like,
    create_activity_undo_like,
    create_activity_announce,
    create_activity_undo_announce,
    create_activity_delete_note,
};
use crate::activitypub::deliverer::deliver_activity;
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{DatabaseError, HttpError, ValidationError};
use crate::ethereum::nft::create_mint_signature;
use crate::ipfs::store as ipfs_store;
use crate::ipfs::posts::PostMetadata;
use crate::ipfs::utils::{IPFS_LOGO, get_ipfs_url};
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::models::attachments::queries::set_attachment_ipfs_cid;
use crate::models::posts::helpers::can_view_post;
use crate::models::posts::mentions::{find_mentioned_profiles, replace_mentions};
use crate::models::posts::tags::{find_tags, replace_tags};
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
use crate::models::posts::types::{PostCreateData, Visibility};
use crate::models::reactions::queries::{
    create_reaction,
    delete_reaction,
};
use super::helpers::{
    get_announce_recipients,
    get_like_recipients,
    get_note_recipients,
    Audience,
};
use super::types::{Status, StatusData, TransactionData};

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
    let mut post_data = PostCreateData::try_from(data.into_inner())?;
    post_data.clean()?;
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
    post_data.mentions.extend(mention_map.values()
        .map(|profile| profile.id));
    post_data.mentions.sort();
    post_data.mentions.dedup();
    // Tags
    post_data.tags = find_tags(&post_data.content);
    post_data.content = replace_tags(
        &instance.url(),
        &post_data.content,
        &post_data.tags,
    );
    // Reply validation
    let maybe_in_reply_to = if let Some(in_reply_to_id) = post_data.in_reply_to_id.as_ref() {
        let in_reply_to = match get_post_by_id(db_client, in_reply_to_id).await {
            Ok(post) => post,
            Err(DatabaseError::NotFound(_)) => {
                return Err(ValidationError("parent post does not exist").into());
            },
            Err(other_error) => return Err(other_error.into()),
        };
        if post_data.visibility != in_reply_to.visibility {
            return Err(ValidationError("post visibility doesn't match the parent").into());
        };
        if post_data.visibility != Visibility::Public {
            let mut in_reply_to_audience: Vec<_> = in_reply_to.mentions.iter()
                .map(|profile| profile.id).collect();
            in_reply_to_audience.push(in_reply_to.author.id);
            if !post_data.mentions.iter().all(|id| in_reply_to_audience.contains(id)) {
                return Err(ValidationError("audience can't be expanded").into());
            };
        };
        Some(in_reply_to)
    } else {
        None
    };
    // Create post
    let mut post = create_post(db_client, &current_user.id, post_data).await?;
    post.in_reply_to = maybe_in_reply_to.map(|mut in_reply_to| {
        in_reply_to.reply_count += 1;
        Box::new(in_reply_to)
    });
    // Federate
    let activity = create_activity_note(
        &instance.host(),
        &instance.url(),
        &post,
    );
    let recipients = get_note_recipients(db_client, &current_user, &post).await?;
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
    if !can_view_post(db_client, maybe_current_user.as_ref(), &post).await? {
        return Err(HttpError::NotFoundError("post"));
    };
    get_reposted_posts(db_client, vec![&mut post]).await?;
    if let Some(user) = maybe_current_user {
        get_actions_for_posts(db_client, &user.id, vec![&mut post]).await?;
    }
    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
}

#[delete("/{status_id}")]
async fn delete_status(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let post = get_post_by_id(db_client, &status_id).await?;
    if post.author.id != current_user.id {
        return Err(HttpError::PermissionError);
    };
    let deletion_queue = delete_post(db_client, &status_id).await?;
    let config_clone = config.clone();
    actix_rt::spawn(async move {
        deletion_queue.process(&config_clone).await;
    });

    let activity = create_activity_delete_note(
        &config.instance_url(),
        &post,
    );
    let recipients = get_note_recipients(db_client, &current_user, &post).await?;
    deliver_activity(&config, &current_user, activity, recipients);

    Ok(HttpResponse::NoContent().finish())
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
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if !post.is_public() {
        return Err(HttpError::NotFoundError("post"));
    };
    let maybe_reaction_created = match create_reaction(
        db_client, &current_user.id, &status_id, None,
    ).await {
        Ok(reaction) => {
            post.reaction_count += 1;
            Some(reaction)
        },
        Err(DatabaseError::AlreadyExists(_)) => None, // post already favourited
        Err(other_error) => return Err(other_error.into()),
    };
    get_reposted_posts(db_client, vec![&mut post]).await?;
    get_actions_for_posts(db_client, &current_user.id, vec![&mut post]).await?;

    if let Some(reaction) = maybe_reaction_created {
        // Federate
        let Audience { recipients, primary_recipient } =
            get_like_recipients(db_client, &config.instance_url(), &post).await?;
        let note_id = post.get_object_id(&config.instance_url());
        let activity = create_activity_like(
            &config.instance_url(),
            &current_user.profile,
            &note_id,
            &reaction.id,
            &primary_recipient,
        );
        deliver_activity(&config, &current_user, activity, recipients);
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
    let mut post = get_post_by_id(db_client, &status_id).await?;
    let maybe_reaction_deleted = match delete_reaction(
        db_client, &current_user.id, &status_id,
    ).await {
        Ok(reaction_id) => {
            post.reaction_count -= 1;
            Some(reaction_id)
        },
        Err(DatabaseError::NotFound(_)) => None, // post not favourited
        Err(other_error) => return Err(other_error.into()),
    };
    get_reposted_posts(db_client, vec![&mut post]).await?;
    get_actions_for_posts(db_client, &current_user.id, vec![&mut post]).await?;

    if let Some(reaction_id) = maybe_reaction_deleted {
        // Federate
        let Audience { recipients, primary_recipient } =
            get_like_recipients(db_client, &config.instance_url(), &post).await?;
        let activity = create_activity_undo_like(
            &config.instance_url(),
            &current_user.profile,
            &reaction_id,
            &primary_recipient,
        );
        deliver_activity(&config, &current_user, activity, recipients);
    };

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
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if !post.is_public() {
        return Err(HttpError::NotFoundError("post"));
    };
    let repost_data = PostCreateData {
        repost_of_id: Some(status_id),
        ..Default::default()
    };
    let repost = create_post(db_client, &current_user.id, repost_data).await?;
    post.repost_count += 1;
    get_reposted_posts(db_client, vec![&mut post]).await?;
    get_actions_for_posts(db_client, &current_user.id, vec![&mut post]).await?;

    // Federate
    let Audience { recipients, .. } =
        get_announce_recipients(db_client, &config.instance_url(), &current_user, &post).await?;
    let activity = create_activity_announce(
        &config.instance_url(),
        &current_user.profile,
        &post,
        &repost.id,
    );
    deliver_activity(&config, &current_user, activity, recipients);

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

    // Federate
    let Audience { recipients, primary_recipient } =
        get_announce_recipients(db_client, &config.instance_url(), &current_user, &post).await?;
    let activity = create_activity_undo_announce(
        &config.instance_url(),
        &current_user.profile,
        repost_id,
        &primary_recipient,
    );
    deliver_activity(&config, &current_user, activity, recipients);

    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
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
    if post.ipfs_cid.is_some() {
        return Err(HttpError::OperationError("post already saved to IPFS"));
    };
    if post.author.id != current_user.id || !post.is_public() {
        // Users can only archive their own public posts
        return Err(HttpError::PermissionError);
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
    let post_url = post.get_object_id(&config.instance_url());
    let post_metadata = PostMetadata::new(
        &post.id,
        &post_url,
        &post.content,
        &post.created_at,
        &post_image_cid,
    );
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
    let wallet_address = current_user.wallet_address
        .ok_or(HttpError::PermissionError)?;
    let post = get_post_by_id(db_client, &status_id).await?;
    if post.author.id != current_user.id || !post.is_public() {
        // Users can only tokenize their own public posts
        return Err(HttpError::PermissionError);
    };
    let ipfs_cid = post.ipfs_cid
        // Post metadata is not immutable
        .ok_or(HttpError::OperationError("post is not immutable"))?;
    let token_uri = get_ipfs_url(&ipfs_cid);
    let signature = create_mint_signature(
        contract_config,
        &wallet_address,
        &token_uri,
    ).map_err(|_| HttpError::InternalError)?;
    Ok(HttpResponse::Ok().json(signature))
}

#[post("/{status_id}/token_minted")]
async fn token_minted(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(status_id): web::Path<Uuid>,
    data: web::Json<TransactionData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let mut post = get_post_by_id(db_client, &status_id).await?;
    if post.token_tx_id.is_some() {
        return Err(HttpError::OperationError("transaction is already registered"));
    };
    if post.author.id != current_user.id || !post.is_public() {
        return Err(HttpError::PermissionError);
    };
    post.token_tx_id = Some(data.into_inner().transaction_id);
    update_post(db_client, &post).await?;
    get_reposted_posts(db_client, vec![&mut post]).await?;
    get_actions_for_posts(db_client, &current_user.id, vec![&mut post]).await?;
    let status = Status::from_post(post, &config.instance_url());
    Ok(HttpResponse::Ok().json(status))
}

pub fn status_api_scope() -> Scope {
    web::scope("/api/v1/statuses")
        // Routes without status ID
        .service(create_status)
        // Routes with status ID
        .service(get_status)
        .service(delete_status)
        .service(get_context)
        .service(favourite)
        .service(unfavourite)
        .service(reblog)
        .service(unreblog)
        .service(make_permanent)
        .service(get_signature)
        .service(token_minted)
}
