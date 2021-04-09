use actix_session::Session;
use actix_web::{
    get, post, web,
    HttpResponse,
};

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{HttpError, ValidationError};
use crate::models::users::queries::{
    is_valid_invite_code,
    create_user,
    get_user_by_wallet_address,
};
use crate::models::users::types::{
    UserRegistrationData,
    UserLoginData,
};
use crate::utils::crypto::{
    hash_password,
    verify_password,
    generate_private_key,
    serialize_private_key,
};
use super::auth::get_current_user;
use super::types::ApiUser;

// /api/v1/accounts
#[post("/api/v0/create")]
async fn create_user_view(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    form: web::Json<UserRegistrationData>,
    session: Session,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    // Validate
    form.clean()?;
    if !config.registrations_open {
        let invite_code = form.invite_code.as_ref()
            .ok_or(ValidationError("invite code is required"))?;
        if !is_valid_invite_code(db_client, &invite_code).await? {
            Err(ValidationError("invalid invite code"))?;
        }
    }
    // Hash password and generate private key
    let password_hash = hash_password(&form.signature)
        .map_err(|_| HttpError::InternalError)?;
    let private_key = match web::block(move || generate_private_key()).await {
        Ok(private_key) => private_key,
        Err(_) => return Err(HttpError::InternalError),
    };
    let private_key_pem = serialize_private_key(private_key)
        .map_err(|_| HttpError::InternalError)?;

    let user = create_user(
        db_client,
        form.into_inner(),
        password_hash,
        private_key_pem,
    ).await?;
    session.set("id", user.id)?;
    let api_user = ApiUser::from_user(user, &config.instance_url());
    Ok(HttpResponse::Created().json(api_user))
}

#[post("/api/v0/login")]
async fn login_view(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    form: web::Json<UserLoginData>,
    session: Session,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_wallet_address(db_client, &form.wallet_address).await?;
    let result = verify_password(&user.password_hash, &form.signature)
        .map_err(|_| ValidationError("incorrect password"))?;
    if !result {
        // Invalid signature/password
        Err(ValidationError("incorrect password"))?;
    }
    session.set("id", &user.id)?;
    let api_user = ApiUser::from_user(user, &config.instance_url());
    Ok(HttpResponse::Ok().json(api_user))
}

#[get("/api/v0/current-user")]
async fn current_user_view(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    session: Session,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_current_user(db_client, session).await?;
    let api_user = ApiUser::from_user(user, &config.instance_url());
    Ok(HttpResponse::Ok().json(api_user))
}

#[post("/api/v0/logout")]
async fn logout_view(
    session: Session,
) -> Result<HttpResponse, HttpError> {
    session.clear();
    Ok(HttpResponse::Ok().body("logged out"))
}
