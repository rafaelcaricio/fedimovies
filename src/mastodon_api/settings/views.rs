use actix_web::{
    dev::ConnectionInfo,
    get,
    post,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use mitra_config::Config;
use mitra_utils::passwords::hash_password;

use crate::database::{get_database_client, DatabaseError, DbPool};
use crate::errors::ValidationError;
use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::types::Account,
    errors::MastodonError,
    oauth::auth::get_current_user,
};
use crate::models::{
    profiles::helpers::find_aliases,
    profiles::queries::get_profile_by_remote_actor_id,
    users::queries::set_user_password,
};
use super::helpers::{
    export_followers,
    export_follows,
    import_follows_task,
    move_followers_task,
    parse_address_list,
};
use super::types::{
    ImportFollowsRequest,
    MoveFollowersRequest,
    PasswordChangeRequest,
};

#[post("/change_password")]
async fn change_password_view(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    request_data: web::Json<PasswordChangeRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let password_hash = hash_password(&request_data.new_password)
        .map_err(|_| MastodonError::InternalError)?;
    set_user_password(db_client, &current_user.id, password_hash).await?;
    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[get("/export_followers")]
async fn export_followers_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let csv = export_followers(
        db_client,
        &config.instance().hostname(),
        &current_user.id,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type("text/csv")
        .body(csv);
    Ok(response)
}

#[get("/export_follows")]
async fn export_follows_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let csv = export_follows(
        db_client,
        &config.instance().hostname(),
        &current_user.id,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type("text/csv")
        .body(csv);
    Ok(response)
}

#[post("/import_follows")]
async fn import_follows_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    request_data: web::Json<ImportFollowsRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let address_list = parse_address_list(&request_data.follows_csv)?;
    tokio::spawn(async move {
        import_follows_task(
            &config,
            current_user,
            &db_pool,
            address_list,
        ).await.unwrap_or_else(|error| {
            log::error!("import follows: {}", error);
        });
    });
    Ok(HttpResponse::NoContent().finish())
}

#[post("/move_followers")]
async fn move_followers(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    request_data: web::Json<MoveFollowersRequest>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let instance = config.instance();
    if request_data.from_actor_id.starts_with(&instance.url()) {
        return Err(ValidationError("can't move from local actor").into());
    };
    // Existence of actor is not verified because
    // the old profile could have been deleted
    let maybe_from_profile = match get_profile_by_remote_actor_id(
        db_client,
        &request_data.from_actor_id,
    ).await {
        Ok(profile) => Some(profile),
        Err(DatabaseError::NotFound(_)) => None,
        Err(other_error) => return Err(other_error.into()),
    };
    if maybe_from_profile.is_some() {
        // Find known aliases of the current user
        let mut aliases = find_aliases(db_client, &current_user.profile).await?
            .into_iter()
            .map(|profile| profile.actor_id(&instance.url()));
        if !aliases.any(|actor_id| actor_id == request_data.from_actor_id) {
            return Err(ValidationError("old profile is not an alias").into());
        };
    };
    let address_list = parse_address_list(&request_data.followers_csv)?;
    let current_user_clone = current_user.clone();
    tokio::spawn(async move {
        move_followers_task(
            &config,
            &db_pool,
            current_user_clone,
            &request_data.from_actor_id,
            maybe_from_profile,
            address_list,
        ).await.unwrap_or_else(|error| {
            log::error!("move followers: {}", error);
        });
    });

    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &instance.url(),
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

pub fn settings_api_scope() -> Scope {
    web::scope("/api/v1/settings")
        .service(change_password_view)
        .service(export_followers_view)
        .service(export_follows_view)
        .service(import_follows_view)
        .service(move_followers)
}
