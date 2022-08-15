use actix_web::{
    get, patch, post, web,
    HttpRequest, HttpResponse, Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use crate::activitypub::builders::{
    follow::prepare_follow,
    undo_follow::prepare_undo_follow,
    update_person::prepare_update_person,
};
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{DatabaseError, HttpError, ValidationError};
use crate::ethereum::contracts::ContractSet;
use crate::ethereum::eip4361::verify_eip4361_signature;
use crate::ethereum::gate::is_allowed_user;
use crate::ethereum::identity::{
    ETHEREUM_EIP191_PROOF,
    DidPkh,
    create_identity_claim,
    verify_identity_proof,
};
use crate::ethereum::subscriptions::{
    create_subscription_signature,
    is_registered_recipient,
};
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::mastodon_api::pagination::get_paginated_response;
use crate::mastodon_api::statuses::helpers::build_status_list;
use crate::mastodon_api::statuses::types::Status;
use crate::models::posts::queries::get_posts_by_author;
use crate::models::profiles::queries::{
    get_profile_by_id,
    search_profile_by_did,
    update_profile,
};
use crate::models::profiles::types::{
    IdentityProof,
    PaymentOption,
    ProfileUpdateData,
};
use crate::models::relationships::queries::{
    create_follow_request,
    follow,
    get_followers_paginated,
    get_following_paginated,
    hide_replies,
    hide_reposts,
    show_replies,
    show_reposts,
    unfollow,
};
use crate::models::subscriptions::queries::get_incoming_subscriptions;
use crate::models::users::queries::{
    is_valid_invite_code,
    create_user,
    get_user_by_did,
};
use crate::models::users::types::UserCreateData;
use crate::utils::crypto::{
    hash_password,
    generate_private_key,
    serialize_private_key,
};
use crate::utils::files::FileError;
use super::helpers::get_relationship;
use super::types::{
    Account,
    AccountCreateData,
    AccountUpdateData,
    FollowData,
    FollowListQueryParams,
    IdentityClaim,
    IdentityClaimQueryParams,
    IdentityProofData,
    RelationshipQueryParams,
    SearchDidQueryParams,
    StatusListQueryParams,
    SubscriptionQueryParams,
    ApiSubscription,
};

#[post("")]
pub async fn create_account(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    maybe_blockchain: web::Data<Option<ContractSet>>,
    account_data: web::Json<AccountCreateData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    // Validate
    account_data.clean()?;
    if !config.registrations_open {
        let invite_code = account_data.invite_code.as_ref()
            .ok_or(ValidationError("invite code is required"))?;
        if !is_valid_invite_code(db_client, invite_code).await? {
            return Err(ValidationError("invalid invite code").into());
        };
    };

    let maybe_password_hash = if let Some(password) = account_data.password.as_ref() {
        let password_hash = hash_password(password)
            .map_err(|_| HttpError::InternalError)?;
        Some(password_hash)
    } else {
        None
    };
    let maybe_wallet_address = if let Some(message) = account_data.message.as_ref() {
        let signature = account_data.signature.as_ref()
            .ok_or(ValidationError("signature is required"))?;
        let wallet_address = verify_eip4361_signature(
            message,
            signature,
            &config.instance().host(),
            &config.login_message,
        )?;
        Some(wallet_address)
    } else {
        None
    };
    if maybe_wallet_address.is_some() == maybe_password_hash.is_some() {
        // Either password or EIP-4361 auth must be used (but not both)
        return Err(ValidationError("invalid login data").into());
    };

    if let Some(contract_set) = maybe_blockchain.as_ref() {
        // Wallet address is required if blockchain integration is enabled
        let wallet_address = maybe_wallet_address.as_ref()
            .ok_or(ValidationError("wallet address is required"))?;
        let is_allowed = is_allowed_user(contract_set, wallet_address).await
            .map_err(|_| HttpError::InternalError)?;
        if !is_allowed {
            return Err(ValidationError("not allowed to sign up").into());
        };
    } else {
        assert!(config.blockchain.is_none());
    };

    // Generate RSA private key for actor
    let private_key = match web::block(generate_private_key).await {
        Ok(Ok(private_key)) => private_key,
        _ => return Err(HttpError::InternalError),
    };
    let private_key_pem = serialize_private_key(&private_key)
        .map_err(|_| HttpError::InternalError)?;

    let AccountCreateData { username, invite_code, .. } =
        account_data.into_inner();
    let user_data = UserCreateData {
        username,
        password_hash: maybe_password_hash,
        private_key_pem,
        wallet_address: maybe_wallet_address,
        invite_code,
    };
    let user = match create_user(db_client, user_data).await {
        Ok(user) => user,
        Err(DatabaseError::AlreadyExists(_)) =>
            return Err(ValidationError("user already exists").into()),
        Err(other_error) => return Err(other_error.into()),
    };
    log::warn!("created user {}", user.id);
    let account = Account::from_user(user, &config.instance_url());
    Ok(HttpResponse::Created().json(account))
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
    account_data: web::Json<AccountUpdateData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let mut profile_data = account_data.into_inner()
        .into_profile_data(
            &current_user.profile.avatar_file_name,
            &current_user.profile.banner_file_name,
            &current_user.profile.identity_proofs.into_inner(),
            &current_user.profile.payment_options.into_inner(),
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
    prepare_update_person(db_client, config.instance(), &current_user).await?
        .spawn_deliver();

    let account = Account::from_user(current_user, &config.instance_url());
    Ok(HttpResponse::Ok().json(account))
}

#[get("/identity_proof")]
async fn get_identity_claim(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    query_params: web::Query<IdentityClaimQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let actor_id = current_user.profile.actor_id(&config.instance_url());
    let did = query_params.did.parse::<DidPkh>()
        .map_err(|_| ValidationError("invalid DID"))?;
    let claim = create_identity_claim(&actor_id, &did)
        .map_err(|_| HttpError::InternalError)?;
    let response = IdentityClaim { claim };
    Ok(HttpResponse::Ok().json(response))
}

#[post("/identity_proof")]
async fn create_identity_proof(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    proof_data: web::Json<IdentityProofData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let actor_id = current_user.profile.actor_id(&config.instance_url());
    let did = proof_data.did.parse::<DidPkh>()
        .map_err(|_| ValidationError("invalid DID"))?;
    if did.currency() != Some(config.default_currency()) {
        return Err(ValidationError("unsupported chain ID").into());
    };
    let maybe_public_address =
        current_user.public_wallet_address(&config.default_currency());
    if let Some(address) = maybe_public_address {
        if did.address != address {
            return Err(ValidationError("DID doesn't match current identity").into());
        };
    };
    match get_user_by_did(db_client, &did).await {
        Ok(user) => {
            if user.id != current_user.id {
                return Err(ValidationError("DID already associated with another user").into());
            };
        },
        Err(DatabaseError::NotFound(_)) => (),
        Err(other_error) => return Err(other_error.into()),
    };
    verify_identity_proof(
        &actor_id,
        &did,
        &proof_data.signature,
    )?;
    let proof = IdentityProof {
        issuer: did,
        proof_type: ETHEREUM_EIP191_PROOF.to_string(),
        value: proof_data.signature.clone(),
    };
    let mut profile_data = ProfileUpdateData::from(&current_user.profile);
    match profile_data.identity_proofs.iter_mut()
            .find(|item| item.issuer == proof.issuer) {
        Some(mut item) => {
            // Replace
            item.proof_type = proof.proof_type;
            item.value = proof.value;
        },
        None => {
            // Add new proof
            profile_data.identity_proofs.push(proof);
        },
    };
    current_user.profile = update_profile(
        db_client,
        &current_user.id,
        profile_data,
    ).await?;

    // Federate
    prepare_update_person(db_client, config.instance(), &current_user).await?
        .spawn_deliver();

    let account = Account::from_user(current_user, &config.instance_url());
    Ok(HttpResponse::Ok().json(account))
}

#[get("/authorize_subscription")]
async fn authorize_subscription(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    query_params: web::Query<SubscriptionQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let ethereum_config = config.blockchain.as_ref()
        .ok_or(HttpError::NotSupported)?
        .ethereum_config()
        .ok_or(HttpError::NotSupported)?;
    // The user must have a public wallet address,
    // because subscribers should be able
    // to verify that payments are actually sent to the recipient.
    let wallet_address = current_user
        .public_wallet_address(&config.default_currency())
        .ok_or(HttpError::PermissionError)?;
    let signature = create_subscription_signature(
        ethereum_config,
        &wallet_address,
        query_params.price,
    ).map_err(|_| HttpError::InternalError)?;
    Ok(HttpResponse::Ok().json(signature))
}

#[post("/subscriptions_enabled")]
async fn subscriptions_enabled(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    maybe_blockchain: web::Data<Option<ContractSet>>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    let contract_set = maybe_blockchain.as_ref().as_ref()
        .ok_or(HttpError::NotSupported)?;
    let wallet_address = current_user
        .public_wallet_address(&config.default_currency())
        .ok_or(HttpError::PermissionError)?;
    let is_registered = is_registered_recipient(contract_set, &wallet_address)
        .await.map_err(|_| HttpError::InternalError)?;
    if !is_registered {
        return Err(ValidationError("recipient is not registered").into());
    };

    if current_user.profile.payment_options.is_empty() {
        // Add payment option to profile
        let mut profile_data = ProfileUpdateData::from(&current_user.profile);
        profile_data.payment_options = vec![PaymentOption::subscription()];
        current_user.profile = update_profile(
            db_client,
            &current_user.id,
            profile_data,
        ).await?;

        // Federate
        prepare_update_person(db_client, config.instance(), &current_user)
            .await?.spawn_deliver();
    };

    let account = Account::from_user(current_user, &config.instance_url());
    Ok(HttpResponse::Ok().json(account))
}

#[get("/relationships")]
async fn get_relationships_view(
    auth: BearerAuth,
    db_pool: web::Data<Pool>,
    query_params: web::Query<RelationshipQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let relationship = get_relationship(
        db_client,
        &current_user.id,
        &query_params.id,
    ).await?;
    Ok(HttpResponse::Ok().json(vec![relationship]))
}

#[get("/search_did")]
async fn search_by_did(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    query_params: web::Query<SearchDidQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let did: DidPkh = query_params.did.parse()
        .map_err(|_| ValidationError("invalid DID"))?;
    let profiles = search_profile_by_did(db_client, &did, false).await?;
    let accounts: Vec<Account> = profiles.into_iter()
        .map(|profile| Account::from_profile(profile, &config.instance_url()))
        .collect();
    Ok(HttpResponse::Ok().json(accounts))
}

#[get("/{account_id}")]
async fn get_account(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    let account = Account::from_profile(profile, &config.instance_url());
    Ok(HttpResponse::Ok().json(account))
}

#[post("/{account_id}/follow")]
async fn follow_account(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    account_id: web::Path<Uuid>,
    follow_data: web::Json<FollowData>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, &account_id).await?;
    if let Some(remote_actor) = target.actor_json {
        // Create follow request if target is remote
        match create_follow_request(db_client, &current_user.id, &target.id).await {
            Ok(follow_request) => {
                prepare_follow(
                    config.instance(),
                    &current_user,
                    &remote_actor,
                    &follow_request.id,
                ).spawn_deliver();
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
    account_id: web::Path<Uuid>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let target = get_profile_by_id(db_client, &account_id).await?;
    match unfollow(db_client, &current_user.id, &target.id).await {
        Ok(Some(follow_request_id)) => {
            // Remote follow
            let remote_actor = target.actor_json
                .ok_or(HttpError::InternalError)?;
            prepare_undo_follow(
                config.instance(),
                &current_user,
                &remote_actor,
                &follow_request_id,
            ).spawn_deliver();
        },
        Ok(None) => (), // local follow
        Err(DatabaseError::NotFound(_)) => (), // not following
        Err(other_error) => return Err(other_error.into()),
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
    account_id: web::Path<Uuid>,
    query_params: web::Query<StatusListQueryParams>,
) -> Result<HttpResponse, HttpError> {
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
        query_params.limit,
    ).await?;
    let statuses = build_status_list(
        db_client,
        &config.instance_url(),
        maybe_current_user.as_ref(),
        posts,
    ).await?;
    Ok(HttpResponse::Ok().json(statuses))
}

#[get("/{account_id}/followers")]
async fn get_account_followers(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    account_id: web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
    request: HttpRequest,
) -> Result<HttpResponse, HttpError> {
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
        query_params.limit.into(),
    ).await?;
    let max_index = usize::from(query_params.limit.saturating_sub(1));
    let maybe_last_id = followers.get(max_index).map(|item| item.relationship_id);
    let accounts: Vec<Account> = followers.into_iter()
        .map(|item| Account::from_profile(item.profile, &config.instance_url()))
        .collect();
    let response = get_paginated_response(
        &config.instance_url(),
        request.uri().path(),
        accounts,
        maybe_last_id,
    );
    Ok(response)
}

#[get("/{account_id}/following")]
async fn get_account_following(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    account_id: web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
    request: HttpRequest,
) -> Result<HttpResponse, HttpError> {
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
        query_params.limit.into(),
    ).await?;
    let max_index = usize::from(query_params.limit.saturating_sub(1));
    let maybe_last_id = following.get(max_index).map(|item| item.relationship_id);
    let accounts: Vec<Account> = following.into_iter()
        .map(|item| Account::from_profile(item.profile, &config.instance_url()))
        .collect();
    let response = get_paginated_response(
        &config.instance_url(),
        request.uri().path(),
        accounts,
        maybe_last_id,
    );
    Ok(response)
}

#[get("/{account_id}/subscribers")]
async fn get_account_subscribers(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    account_id: web::Path<Uuid>,
    query_params: web::Query<FollowListQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let profile = get_profile_by_id(db_client, &account_id).await?;
    if profile.id != current_user.id {
        // Social graph is hidden
        let subscriptions: Vec<ApiSubscription> = vec![];
        return Ok(HttpResponse::Ok().json(subscriptions));
    };
    let instance_url = config.instance_url();
    let subscriptions: Vec<ApiSubscription> = get_incoming_subscriptions(
        db_client,
        &profile.id,
        query_params.max_id,
        query_params.limit.into(),
    )
        .await?
        .into_iter()
        .map(|item| ApiSubscription::from_subscription(&instance_url, item))
        .collect();
    Ok(HttpResponse::Ok().json(subscriptions))
}

pub fn account_api_scope() -> Scope {
    web::scope("/api/v1/accounts")
        // Routes without account ID
        .service(create_account)
        .service(verify_credentials)
        .service(update_credentials)
        .service(get_identity_claim)
        .service(create_identity_proof)
        .service(authorize_subscription)
        .service(subscriptions_enabled)
        .service(get_relationships_view)
        .service(search_by_did)
        // Routes with account ID
        .service(get_account)
        .service(follow_account)
        .service(unfollow_account)
        .service(get_account_statuses)
        .service(get_account_followers)
        .service(get_account_following)
        .service(get_account_subscribers)
}
