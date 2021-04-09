use actix_session::Session;
use actix_web::{get, post, patch, web, HttpResponse, Scope};
use serde::Deserialize;
use uuid::Uuid;

use crate::activitypub::activity::{
    create_activity_follow,
    create_activity_undo_follow,
};
use crate::activitypub::actor::Actor;
use crate::activitypub::deliverer::deliver_activity;
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::HttpError;
use crate::mastodon_api::statuses::types::Status;
use crate::mastodon_api::users::auth::get_current_user;
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

#[patch("/update_credentials")]
async fn update_credentials(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    session: Session,
    data: web::Json<AccountUpdateData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, session).await?;
    let profile = get_profile_by_id(db_client, &current_user.id).await?;
    let mut profile_data = data.into_inner()
        .into_profile_data(
            &profile.avatar_file_name,
            &profile.banner_file_name,
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
    let updated_profile = update_profile(
        db_client,
        &profile.id,
        profile_data,
    ).await?;
    let account = Account::from_profile(updated_profile, &config.instance_url());
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
    db_pool: web::Data<Pool>,
    session: Session,
    query_params: web::Query<RelationshipQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, session).await?;
    let relationships = follows::get_relationships(
        db_client,
        current_user.id,
        vec![query_params.into_inner().id],
    ).await?;
    Ok(HttpResponse::Ok().json(relationships))
}

#[post("/{account_id}/follow")]
async fn follow(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    session: Session,
    web::Path(account_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, session).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    let relationship = if let Some(actor_value) = profile.actor_json {
        // Remote follow
        let request = follows::create_follow_request(db_client, &current_user.id, &profile.id).await?;
        let actor: Actor = serde_json::from_value(actor_value)
            .map_err(|_| HttpError::InternalError)?;
        let activity = create_activity_follow(
            &config,
            &current_user.profile,
            &request.id,
            &actor.id,
        );
        let activity_sender = current_user.clone();
        actix_rt::spawn(async move {
            deliver_activity(
                &config,
                &activity_sender,
                activity,
                vec![actor],
            ).await;
        });
        follows::get_relationship(db_client, &current_user.id, &profile.id).await?
    } else {
        follows::follow(db_client, &current_user.id, &profile.id).await?
    };
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/unfollow")]
async fn unfollow(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    session: Session,
    web::Path(account_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, session).await?;
    let target_profile = get_profile_by_id(db_client, &account_id).await?;
    let relationship = if let Some(actor_value) = target_profile.actor_json {
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
        let actor: Actor = serde_json::from_value(actor_value)
            .map_err(|_| HttpError::InternalError)?;
        let activity = create_activity_undo_follow(
            &config,
            &current_user.profile,
            &follow_request.id,
            &actor.id,
        );
        actix_rt::spawn(async move {
            deliver_activity(
                &config,
                &current_user,
                activity,
                vec![actor],
            ).await;
        });
        // TODO: uncouple unfollow and get_relationship
        relationship
    } else {
        follows::unfollow(db_client, &current_user.id, &target_profile.id).await?
    };
    Ok(HttpResponse::Ok().json(relationship))
}

#[get("/{account_id}/statuses")]
async fn get_account_statuses(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    web::Path(account_id): web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let posts = get_posts_by_author(db_client, &account_id).await?;
    let statuses: Vec<Status> = posts.into_iter()
        .map(|post| Status::from_post(post, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(statuses))
}

pub fn account_api_scope() -> Scope {
    web::scope("/api/v1/accounts")
        // Routes without account ID
        .service(get_relationships)
        .service(update_credentials)
        // Routes with account ID
        .service(get_account)
        .service(follow)
        .service(unfollow)
        .service(get_account_statuses)
}
