use actix_web::{get, post, patch, web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use serde::Deserialize;
use uuid::Uuid;

use crate::activitypub::activity::{
    create_activity_follow,
    create_activity_undo_follow,
};
use crate::activitypub::deliverer::deliver_activity;
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::statuses::types::Status;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::mastodon_api::users::views::create_user_view;
use crate::models::posts::helpers::get_actions_for_posts;
use crate::models::posts::queries::get_posts_by_author;
use crate::models::profiles::queries::{
    get_profile_by_id,
    update_profile,
};
use crate::models::relationships::queries as follows;
use crate::utils::files::FileError;
use super::types::{Account, AccountUpdateData};

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
async fn get_relationships(
    auth: BearerAuth,
    db_pool: web::Data<Pool>,
    query_params: web::Query<RelationshipQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let relationships = follows::get_relationships(
        db_client,
        current_user.id,
        vec![query_params.into_inner().id],
    ).await?;
    Ok(HttpResponse::Ok().json(relationships))
}

#[post("/{account_id}/follow")]
async fn follow(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    let maybe_remote_actor = profile.actor().map_err(|_| HttpError::InternalError)?;
    let relationship = if let Some(remote_actor) = maybe_remote_actor {
        // Remote follow
        let request = follows::create_follow_request(db_client, &current_user.id, &profile.id).await?;
        let activity = create_activity_follow(
            &config.instance_url(),
            &current_user.profile,
            &request.id,
            &remote_actor.id,
        );
        deliver_activity(&config, &current_user, activity, vec![remote_actor]);
        follows::get_relationship(db_client, &current_user.id, &profile.id).await?
    } else {
        follows::follow(db_client, &current_user.id, &profile.id).await?
    };
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/unfollow")]
async fn unfollow(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target_profile = get_profile_by_id(db_client, &account_id).await?;
    let maybe_remote_actor = target_profile.actor().map_err(|_| HttpError::InternalError)?;
    let relationship = if let Some(remote_actor) = maybe_remote_actor {
        // Remote follow
        let follow_request = follows::get_follow_request_by_path(
            db_client,
            &current_user.id,
            &target_profile.id,
        ).await?;
        let relationship = follows::unfollow(
            db_client,
            &current_user.id,
            &target_profile.id,
        ).await?;
        // Federate
        let activity = create_activity_undo_follow(
            &config.instance_url(),
            &current_user.profile,
            &follow_request.id,
            &remote_actor.id,
        );
        deliver_activity(&config, &current_user, activity, vec![remote_actor]);
        // TODO: uncouple unfollow and get_relationship
        relationship
    } else {
        follows::unfollow(db_client, &current_user.id, &target_profile.id).await?
    };
    Ok(HttpResponse::Ok().json(relationship))
}

#[get("/{account_id}/statuses")]
async fn get_account_statuses(
    auth: Option<BearerAuth>,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    let mut posts = get_posts_by_author(db_client, &account_id, false).await?;
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

pub fn account_api_scope() -> Scope {
    web::scope("/api/v1/accounts")
        // Routes without account ID
        .service(create_user_view)
        .service(get_relationships)
        .service(verify_credentials)
        .service(update_credentials)
        // Routes with account ID
        .service(get_account)
        .service(follow)
        .service(unfollow)
        .service(get_account_statuses)
}
