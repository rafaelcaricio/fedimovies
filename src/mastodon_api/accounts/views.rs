use actix_web::{dev::ConnectionInfo, get, patch, post, web, HttpRequest, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use fedimovies_config::{Config, DefaultRole, RegistrationType};
use fedimovies_models::relationships::queries::{mute_posts, unmute_posts};
use fedimovies_models::{
    database::{get_database_client, DatabaseError, DbPool},
    posts::queries::get_posts_by_author,
    profiles::helpers::find_verified_aliases,
    profiles::queries::{
        get_profile_by_acct, get_profile_by_id, search_profiles_by_did, update_profile,
    },
    relationships::queries::{
        get_followers_paginated, get_following_paginated, hide_replies, hide_reposts, show_replies,
        show_reposts, unfollow,
    },
    subscriptions::queries::get_incoming_subscriptions,
    users::queries::{create_user, is_valid_invite_code},
    users::types::{Role, UserCreateData},
};
use fedimovies_utils::{
    canonicalization::canonicalize_object,
    crypto_rsa::{generate_rsa_key, serialize_private_key},
    did::Did,
    id::generate_ulid,
    passwords::hash_password,
};

use super::helpers::{get_aliases, get_relationship};
use super::types::{
    Account, AccountCreateData, AccountUpdateData, ActivityParams, ApiSubscription, FollowData,
    FollowListQueryParams, LookupAcctQueryParams, RelationshipQueryParams, SearchAcctQueryParams,
    SearchDidQueryParams, StatusListQueryParams, UnsignedActivity,
};
use crate::activitypub::builders::{
    follow::follow_or_create_request,
    undo_follow::prepare_undo_follow,
    update_person::{build_update_person, prepare_update_person},
};
use crate::errors::ValidationError;
use crate::http::{get_request_base_url, FormOrJson};
use crate::mastodon_api::{
    errors::MastodonError, oauth::auth::get_current_user, pagination::get_paginated_response,
    search::helpers::search_profiles_only, statuses::helpers::build_status_list,
    statuses::types::Status,
};
use crate::validators::{profiles::clean_profile_update_data, users::validate_local_username};

#[post("")]
pub async fn create_account(
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_data: web::Json<AccountCreateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    // Validate
    if config.registration.registration_type == RegistrationType::Invite {
        let invite_code = account_data
            .invite_code
            .as_ref()
            .ok_or(ValidationError("invite code is required".to_string()))?;
        if !is_valid_invite_code(db_client, invite_code).await? {
            return Err(ValidationError("invalid invite code".to_string()).into());
        };
    };

    validate_local_username(&account_data.username)?;
    if account_data.password.is_none() && account_data.message.is_none() {
        return Err(ValidationError("password or EIP-4361 message is required".to_string()).into());
    };
    let maybe_password_hash = if let Some(password) = account_data.password.as_ref() {
        let password_hash = hash_password(password).map_err(|_| MastodonError::InternalError)?;
        Some(password_hash)
    } else {
        None
    };

    // Generate RSA private key for actor
    let private_key = match web::block(generate_rsa_key).await {
        Ok(Ok(private_key)) => private_key,
        _ => return Err(MastodonError::InternalError),
    };
    let private_key_pem =
        serialize_private_key(&private_key).map_err(|_| MastodonError::InternalError)?;

    let AccountCreateData {
        username,
        invite_code,
        ..
    } = account_data.into_inner();
    let role = match config.registration.default_role {
        DefaultRole::NormalUser => Role::NormalUser,
        DefaultRole::ReadOnlyUser => Role::ReadOnlyUser,
    };
    let user_data = UserCreateData {
        username,
        password_hash: maybe_password_hash,
        private_key_pem,
        wallet_address: None,
        invite_code,
        role,
    };
    let user = match create_user(db_client, user_data).await {
        Ok(user) => user,
        Err(DatabaseError::AlreadyExists(_)) => {
            return Err(ValidationError("user already exists".to_string()).into())
        }
        Err(other_error) => return Err(other_error.into()),
    };
    log::warn!("created user {}", user.id);
    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        user,
    );
    Ok(HttpResponse::Created().json(account))
}

#[get("/verify_credentials")]
async fn verify_credentials(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_current_user(db_client, auth.token()).await?;
    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[patch("/update_credentials")]
async fn update_credentials(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_data: web::Json<AccountUpdateData>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let mut profile_data = account_data
        .into_inner()
        .into_profile_data(&current_user.profile, &config.media_dir())?;
    clean_profile_update_data(&mut profile_data)?;
    current_user.profile = update_profile(db_client, &current_user.id, profile_data).await?;

    // Federate
    prepare_update_person(db_client, &config.instance(), &current_user, None)
        .await?
        .enqueue(db_client)
        .await?;

    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[get("/signed_update")]
async fn get_unsigned_update(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let internal_activity_id = generate_ulid();
    let activity = build_update_person(
        &config.instance_url(),
        &current_user,
        Some(internal_activity_id),
    )
    .map_err(|_| MastodonError::InternalError)?;
    let canonical_json =
        canonicalize_object(&activity).map_err(|_| MastodonError::InternalError)?;
    let data = UnsignedActivity {
        params: ActivityParams::Update {
            internal_activity_id,
        },
        message: canonical_json,
    };
    Ok(HttpResponse::Ok().json(data))
}

#[get("/relationships")]
async fn get_relationships_view(
    auth: BearerAuth,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<RelationshipQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let relationship = get_relationship(db_client, &current_user.id, &query_params.id).await?;
    Ok(HttpResponse::Ok().json(vec![relationship]))
}

#[get("/lookup")]
async fn lookup_acct(
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<LookupAcctQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_acct(db_client, &query_params.acct).await?;
    let account = Account::from_profile(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        profile,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[get("/search")]
async fn search_by_acct(
    auth: Option<BearerAuth>,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<SearchAcctQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    match auth {
        Some(auth) => {
            get_current_user(db_client, auth.token()).await?;
        }
        None => {
            // Only authorized users can make webfinger queries
            if query_params.resolve {
                return Err(MastodonError::PermissionError);
            };
        }
    };
    let profiles = search_profiles_only(
        &config,
        db_client,
        &query_params.q,
        query_params.resolve,
        query_params.limit.inner(),
    )
    .await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = profiles
        .into_iter()
        .map(|profile| Account::from_profile(&base_url, &instance_url, profile))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

#[get("/search_did")]
async fn search_by_did(
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<SearchDidQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let did: Did = query_params
        .did
        .parse()
        .map_err(|_| ValidationError("invalid DID".to_string()))?;
    let profiles = search_profiles_by_did(db_client, &did, false).await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = profiles
        .into_iter()
        .map(|profile| Account::from_profile(&base_url, &instance_url, profile))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

#[get("/{account_id}")]
async fn get_account(
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    let account = Account::from_profile(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        profile,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[post("/{account_id}/follow")]
async fn follow_account(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
    follow_data: FormOrJson<FollowData>,
) -> Result<HttpResponse, MastodonError> {
    let follow_data = follow_data.into_inner();
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, &account_id).await?;
    follow_or_create_request(db_client, &config.instance(), &current_user, &target).await?;
    if follow_data.reblogs {
        show_reposts(db_client, &current_user.id, &target.id).await?;
    } else {
        hide_reposts(db_client, &current_user.id, &target.id).await?;
    };
    if follow_data.replies {
        show_replies(db_client, &current_user.id, &target.id).await?;
    } else {
        hide_replies(db_client, &current_user.id, &target.id).await?;
    };
    let relationship = get_relationship(db_client, &current_user.id, &target.id).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/unfollow")]
async fn unfollow_account(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, &account_id).await?;
    match unfollow(db_client, &current_user.id, &target.id).await {
        Ok(Some(follow_request_id)) => {
            // Remote follow
            let remote_actor = target.actor_json.ok_or(MastodonError::InternalError)?;
            prepare_undo_follow(
                &config.instance(),
                &current_user,
                &remote_actor,
                &follow_request_id,
            )
            .enqueue(db_client)
            .await?;
        }
        Ok(None) => (),                        // local follow
        Err(DatabaseError::NotFound(_)) => (), // not following
        Err(other_error) => return Err(other_error.into()),
    };

    let relationship = get_relationship(db_client, &current_user.id, &target.id).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/mute")]
async fn mute_account(
    auth: BearerAuth,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, &account_id).await?;

    mute_posts(db_client, &current_user.id, &target.id).await?;

    let relationship = get_relationship(db_client, &current_user.id, &target.id).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[post("/{account_id}/unmute")]
async fn unmute_account(
    auth: BearerAuth,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, &account_id).await?;
    match unmute_posts(db_client, &current_user.id, &target.id).await {
        Ok(()) => (),
        Err(DatabaseError::NotFound(_)) => (), // not following
        Err(other_error) => return Err(other_error.into()),
    };

    let relationship = get_relationship(db_client, &current_user.id, &target.id).await?;
    Ok(HttpResponse::Ok().json(relationship))
}

#[get("/{account_id}/statuses")]
async fn get_account_statuses(
    auth: Option<BearerAuth>,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
    query_params: web::Query<StatusListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let maybe_current_user = match auth {
        Some(auth) => Some(get_current_user(db_client, auth.token()).await?),
        None => None,
    };
    if query_params.pinned {
        // Pinned posts are not supported
        let statuses: Vec<Status> = vec![];
        return Ok(HttpResponse::Ok().json(statuses));
    };
    let profile = get_profile_by_id(db_client, &account_id).await?;
    // Include reposts but not replies
    let posts = get_posts_by_author(
        db_client,
        &profile.id,
        maybe_current_user.as_ref().map(|user| &user.id),
        !query_params.exclude_replies,
        true,
        query_params.max_id,
        query_params.limit.inner(),
    )
    .await?;
    let statuses = build_status_list(
        db_client,
        &get_request_base_url(connection_info),
        &config.instance_url(),
        maybe_current_user.as_ref(),
        posts,
    )
    .await?;
    Ok(HttpResponse::Ok().json(statuses))
}

#[get("/{account_id}/followers")]
async fn get_account_followers(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
    request: HttpRequest,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    if profile.id != current_user.id {
        // Social graph is hidden
        let accounts: Vec<Account> = vec![];
        return Ok(HttpResponse::Ok().json(accounts));
    };
    let followers = get_followers_paginated(
        db_client,
        &profile.id,
        query_params.max_id,
        query_params.limit.inner(),
    )
    .await?;
    let max_index = usize::from(query_params.limit.inner().saturating_sub(1));
    let maybe_last_id = followers.get(max_index).map(|item| item.relationship_id);
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = followers
        .into_iter()
        .map(|item| Account::from_profile(&base_url, &instance_url, item.profile))
        .collect();
    let response =
        get_paginated_response(&instance_url, request.uri().path(), accounts, maybe_last_id);
    Ok(response)
}

#[get("/{account_id}/following")]
async fn get_account_following(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
    request: HttpRequest,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    if profile.id != current_user.id {
        // Social graph is hidden
        let accounts: Vec<Account> = vec![];
        return Ok(HttpResponse::Ok().json(accounts));
    };
    let following = get_following_paginated(
        db_client,
        &profile.id,
        query_params.max_id,
        query_params.limit.inner(),
    )
    .await?;
    let max_index = usize::from(query_params.limit.inner().saturating_sub(1));
    let maybe_last_id = following.get(max_index).map(|item| item.relationship_id);
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance().url();
    let accounts: Vec<Account> = following
        .into_iter()
        .map(|item| Account::from_profile(&base_url, &instance_url, item.profile))
        .collect();
    let response =
        get_paginated_response(&instance_url, request.uri().path(), accounts, maybe_last_id);
    Ok(response)
}

#[get("/{account_id}/subscribers")]
async fn get_account_subscribers(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    if profile.id != current_user.id {
        // Social graph is hidden
        let subscriptions: Vec<ApiSubscription> = vec![];
        return Ok(HttpResponse::Ok().json(subscriptions));
    };
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance_url();
    let subscriptions: Vec<ApiSubscription> = get_incoming_subscriptions(
        db_client,
        &profile.id,
        query_params.max_id,
        query_params.limit.inner(),
    )
    .await?
    .into_iter()
    .map(|subscription| ApiSubscription::from_subscription(&base_url, &instance_url, subscription))
    .collect();
    Ok(HttpResponse::Ok().json(subscriptions))
}

#[get("/{account_id}/aliases")]
async fn get_account_aliases(
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    let aliases = find_verified_aliases(db_client, &profile).await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance_url();
    let accounts: Vec<Account> = aliases
        .into_iter()
        .map(|profile| Account::from_profile(&base_url, &instance_url, profile))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

#[get("/{account_id}/aliases/all")]
async fn get_all_account_aliases(
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    let base_url = get_request_base_url(connection_info);
    let instance_url = config.instance_url();
    let aliases = get_aliases(db_client, &base_url, &instance_url, &profile).await?;
    Ok(HttpResponse::Ok().json(aliases))
}

pub fn account_api_scope() -> Scope {
    web::scope("/api/v1/accounts")
        // Routes without account ID
        .service(create_account)
        .service(verify_credentials)
        .service(update_credentials)
        .service(get_unsigned_update)
        .service(get_relationships_view)
        .service(lookup_acct)
        .service(search_by_acct)
        .service(search_by_did)
        // Routes with account ID
        .service(get_account)
        .service(follow_account)
        .service(unfollow_account)
        .service(mute_account)
        .service(unmute_account)
        .service(get_account_statuses)
        .service(get_account_followers)
        .service(get_account_following)
        .service(get_account_subscribers)
        .service(get_account_aliases)
        .service(get_all_account_aliases)
}
