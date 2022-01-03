use actix_web::{get, post, patch, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use serde::Deserialize;
use uuid::Uuid;

use crate::activitypub::activity::{
    create_activity_follow,
    create_activity_undo_follow,
    create_activity_update_person,
};
use crate::activitypub::actor::Actor;
use crate::activitypub::deliverer::deliver_activity;
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{DatabaseError, HttpError, ValidationError};
use crate::ethereum::gate::is_allowed_user;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::models::posts::helpers::{
    get_actions_for_posts,
    get_reposted_posts,
};
use crate::mastodon_api::statuses::types::Status;
use crate::mastodon_api::timelines::types::TimelineQueryParams;
use crate::models::posts::queries::get_posts_by_author;
use crate::models::profiles::queries::{
    get_profile_by_id,
    update_profile,
};
use crate::models::relationships::queries::{
    create_follow_request,
    follow,
    get_follow_request_by_path,
    get_followers,
    get_following,
    get_relationship,
    get_relationships,
    unfollow,
};
use crate::models::users::queries::{
    is_valid_invite_code,
    create_user,
};
use crate::utils::crypto::{
    hash_password,
    generate_private_key,
    serialize_private_key,
};
use crate::utils::files::FileError;
use super::types::{
    Account,
    AccountCreateData,
    AccountUpdateData,
    FollowListQueryParams,
};

#[post("")]
pub async fn create_account(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    account_data: web::Json<AccountCreateData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let user_data = account_data.into_inner().into_user_data();
    // Validate
    user_data.clean()?;
    if !config.registrations_open {
        let invite_code = user_data.invite_code.as_ref()
            .ok_or(ValidationError("invite code is required"))?;
        if !is_valid_invite_code(db_client, invite_code).await? {
            return Err(ValidationError("invalid invite code").into());
        }
    }
    if config.ethereum_contract.is_some() {
        // Wallet address is required only if ethereum integration is enabled
        let wallet_address = user_data.wallet_address.as_ref()
            .ok_or(ValidationError("wallet address is required"))?;
        let is_allowed = is_allowed_user(&config, wallet_address).await
            .map_err(|_| HttpError::InternalError)?;
        if !is_allowed {
            return Err(ValidationError("not allowed to sign up").into());
        }
    }
    // Hash password and generate private key
    let password_hash = hash_password(&user_data.password)
        .map_err(|_| HttpError::InternalError)?;
    let private_key = match web::block(generate_private_key).await {
        Ok(private_key) => private_key,
        Err(_) => return Err(HttpError::InternalError),
    };
    let private_key_pem = serialize_private_key(private_key)
        .map_err(|_| HttpError::InternalError)?;

    let user = create_user(
        db_client,
        user_data,
        password_hash,
        private_key_pem,
    ).await?;
    let account = Account::from_user(user, &config.instance_url());
    Ok(HttpResponse::Created().json(account))
}

#[get("/{account_id}")]
async fn get_account(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    let account = Account::from_profile(profile, &config.instance_url());
    Ok(HttpResponse::Ok().json(account))
}

#[get("/verify_credentials")]
async fn verify_credentials(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_current_user(db_client, auth.token()).await?;
    let account = Account::from_user(user, &config.instance_url());
    Ok(HttpResponse::Ok().json(account))
}

#[patch("/update_credentials")]
async fn update_credentials(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    data: web::Json<AccountUpdateData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let mut profile_data = data.into_inner()
        .into_profile_data(
            &current_user.profile.avatar_file_name,
            &current_user.profile.banner_file_name,
            &config.media_dir(),
        )
        .map_err(|err| {
            match err {
                FileError::Base64DecodingError(_) => {
                    HttpError::ValidationError("base64 decoding error".into())
                },
                FileError::InvalidMediaType => {
                    HttpError::ValidationError("invalid media type".into())
                },
                _ => HttpError::InternalError,
            }
        })?;
    profile_data.clean()?;
    current_user.profile = update_profile(
        db_client,
        &current_user.id,
        profile_data,
    ).await?;

    // Federate
    let activity = create_activity_update_person(&current_user, &config.instance_url())
        .map_err(|_| HttpError::InternalError)?;
    let followers = get_followers(db_client, &current_user.id, None, None).await?;
    let mut recipients: Vec<Actor> = Vec::new();
    for follower in followers {
        if let Some(remote_actor) = follower.actor_json {
            recipients.push(remote_actor);
        };
    };
    deliver_activity(&config, &current_user, activity, recipients);

    let account = Account::from_user(current_user, &config.instance_url());
    Ok(HttpResponse::Ok().json(account))
}

// TODO: actix currently doesn't support parameter arrays
// https://github.com/actix/actix-web/issues/2044
#[derive(Deserialize)]
pub struct RelationshipQueryParams {
    #[serde(rename(deserialize = "id[]"))]
    id: Uuid,
}

#[get("/relationships")]
async fn get_relationships_view(
    auth: BearerAuth,
    db_pool: web::Data<Pool>,
    query_params: web::Query<RelationshipQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let relationships = get_relationships(
        db_client,
        current_user.id,
        vec![query_params.into_inner().id],
    ).await?;
    Ok(HttpResponse::Ok().json(relationships))
}

#[post("/{account_id}/follow")]
async fn follow_account(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, &account_id).await?;
    if let Some(remote_actor) = target.actor_json {
        // Remote follow
        match create_follow_request(db_client, &current_user.id, &target.id).await {
            Ok(request) => {
                let activity = create_activity_follow(
                    &config.instance_url(),
                    &current_user.profile,
                    &request.id,
                    &remote_actor.id,
                );
                deliver_activity(&config, &current_user, activity, vec![remote_actor]);
            },
            Err(DatabaseError::AlreadyExists(_)) => (), // already following
            Err(other_error) => return Err(other_error.into()),
        };
    } else {
        match follow(db_client, &current_user.id, &target.id).await {
            Ok(_) => (),
            Err(DatabaseError::AlreadyExists(_)) => (), // already following
            Err(other_error) => return Err(other_error.into()),
        };
    };
    let relationship = get_relationship(
        db_client,
        &current_user.id,
        &target.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/unfollow")]
async fn unfollow_account(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, &account_id).await?;
    if let Some(remote_actor) = target.actor_json {
        // Remote follow
        match get_follow_request_by_path(
            db_client,
            &current_user.id,
            &target.id,
        ).await {
            Ok(follow_request) => {
                unfollow(
                    db_client,
                    &current_user.id,
                    &target.id,
                ).await?;
                // Federate
                let activity = create_activity_undo_follow(
                    &config.instance_url(),
                    &current_user.profile,
                    &follow_request.id,
                    &remote_actor.id,
                );
                deliver_activity(&config, &current_user, activity, vec![remote_actor]);
            },
            Err(DatabaseError::NotFound(_)) => (), // not following
            Err(other_error) => return Err(other_error.into()),
        };
    } else {
        match unfollow(db_client, &current_user.id, &target.id).await {
            Ok(_) => (),
            Err(DatabaseError::NotFound(_)) => (), // not following
            Err(other_error) => return Err(other_error.into()),
        };
    };
    let relationship = get_relationship(
        db_client,
        &current_user.id,
        &target.id,
    ).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[get("/{account_id}/statuses")]
async fn get_account_statuses(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
    query_params: web::Query<TimelineQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let mut posts = get_posts_by_author(
        db_client,
        &account_id,
        false,
        false,
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
    }
    let statuses: Vec<Status> = posts.into_iter()
        .map(|post| Status::from_post(post, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(statuses))
}

#[get("/{account_id}/followers")]
async fn get_account_followers(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    if profile.id != current_user.id {
        // Social graph is hidden
        let accounts: Vec<Account> = vec![];
        return Ok(HttpResponse::Ok().json(accounts));
    };
    let followers = get_followers(
        db_client,
        &profile.id,
        query_params.max_id,
        Some(query_params.limit),
    ).await?;
    let accounts: Vec<Account> = followers.into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

#[get("/{account_id}/following")]
async fn get_account_following(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    if profile.id != current_user.id {
        // Social graph is hidden
        let accounts: Vec<Account> = vec![];
        return Ok(HttpResponse::Ok().json(accounts));
    };
    let following = get_following(
        db_client,
        &profile.id,
        query_params.max_id,
        Some(query_params.limit),
    ).await?;
    let accounts: Vec<Account> = following.into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

pub fn account_api_scope() -> Scope {
    web::scope("/api/v1/accounts")
        // Routes without account ID
        .service(create_account)
        .service(get_relationships_view)
        .service(verify_credentials)
        .service(update_credentials)
        // Routes with account ID
        .service(get_account)
        .service(follow_account)
        .service(unfollow_account)
        .service(get_account_statuses)
        .service(get_account_followers)
        .service(get_account_following)
}
