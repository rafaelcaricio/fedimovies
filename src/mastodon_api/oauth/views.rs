use actix_web::{
    get,
    post,
    web,
    Either,
    HttpResponse,
    Scope as ActixScope,
    http::header as http_header,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use chrono::{Duration, Utc};

use crate::config::Config;
use crate::database::{get_database_client, DatabaseError, DbPool};
use crate::errors::{HttpError, ValidationError};
use crate::ethereum::eip4361::verify_eip4361_signature;
use crate::models::oauth::queries::{
    create_oauth_authorization,
    delete_oauth_token,
    get_oauth_app_by_client_id,
    get_user_by_authorization_code,
    save_oauth_token,
};
use crate::models::users::queries::{
    get_user_by_name,
    get_user_by_login_address,
};
use crate::utils::currencies::{validate_wallet_address, Currency};
use crate::utils::passwords::verify_password;
use super::auth::get_current_user;
use super::types::{
    AuthorizationRequest,
    AuthorizationQueryParams,
    RevocationRequest,
    TokenRequest,
    TokenResponse,
};
use super::utils::{
    generate_access_token,
    render_authorization_page,
};

#[get("/authorize")]
async fn authorization_page_view() -> HttpResponse {
    let page = render_authorization_page();
    HttpResponse::Ok()
        .content_type("text/html")
        .body(page)
}

const AUTHORIZATION_CODE_EXPIRES_IN: i64 = 86400 * 30;

#[post("/authorize")]
async fn authorize_view(
    db_pool: web::Data<DbPool>,
    form_data: web::Form<AuthorizationRequest>,
    query_params: web::Query<AuthorizationQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &form_data.username).await?;
    let password_hash = user.password_hash.as_ref()
        .ok_or(ValidationError("password auth is disabled"))?;
    let password_correct = verify_password(
        password_hash,
        &form_data.password,
    ).map_err(|_| HttpError::InternalError)?;
    if !password_correct {
        return Err(ValidationError("incorrect password").into());
    };
    if query_params.response_type != "code" {
        return Err(ValidationError("invalid response type").into());
    };
    let oauth_app = get_oauth_app_by_client_id(
        db_client,
        &query_params.client_id,
    ).await?;
    if oauth_app.redirect_uri != query_params.redirect_uri {
        return Err(ValidationError("invalid redirect_uri parameter").into());
    };

    let authorization_code = generate_access_token();
    let created_at = Utc::now();
    let expires_at = created_at + Duration::seconds(AUTHORIZATION_CODE_EXPIRES_IN);
    create_oauth_authorization(
        db_client,
        &authorization_code,
        &user.id,
        oauth_app.id,
        &query_params.scope.replace('+', " "),
        &created_at,
        &expires_at,
    ).await?;

    let redirect_uri = format!(
        "{}?code={}",
        oauth_app.redirect_uri,
        authorization_code,
    );
    let response = HttpResponse::Found()
        .append_header((http_header::LOCATION, redirect_uri))
        .finish();
    Ok(response)
}

const ACCESS_TOKEN_EXPIRES_IN: i64 = 86400 * 7;

/// OAuth 2.0 Password Grant
/// https://oauth.net/2/grant-types/password/
#[post("/token")]
async fn token_view(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    request_data: Either<
        web::Json<TokenRequest>,
        web::Form<TokenRequest>,
    >,
) -> Result<HttpResponse, HttpError> {
    let request_data = match request_data {
        Either::Left(json) => json.into_inner(),
        Either::Right(form) => form.into_inner(),
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let user = match request_data.grant_type.as_str() {
        "authorization_code" => {
            let authorization_code = request_data.code.as_ref()
                .ok_or(ValidationError("authorization code is required"))?;
            get_user_by_authorization_code(
                db_client,
                authorization_code,
            ).await?
        },
        "password" => {
            let username = request_data.username.as_ref()
                .ok_or(ValidationError("username is required"))?;
            get_user_by_name(db_client, username).await?
        },
        "ethereum" => {
            // DEPRECATED
            let wallet_address = request_data.wallet_address.as_ref()
                .ok_or(ValidationError("wallet address is required"))?;
            validate_wallet_address(&Currency::Ethereum, wallet_address)?;
            get_user_by_login_address(db_client, wallet_address).await?
        },
        "eip4361" => {
            let message = request_data.message.as_ref()
                .ok_or(ValidationError("message is required"))?;
            let signature = request_data.signature.as_ref()
                .ok_or(ValidationError("signature is required"))?;
            let wallet_address = verify_eip4361_signature(
                message,
                signature,
                &config.instance().hostname(),
                &config.login_message,
            )?;
            get_user_by_login_address(db_client, &wallet_address).await?
        },
        _ => {
            return Err(ValidationError("unsupported grant type").into());
        },
    };
    if request_data.grant_type == "password" || request_data.grant_type == "ethereum" {
        let password = request_data.password.as_ref()
            .ok_or(ValidationError("password is required"))?;
        let password_hash = user.password_hash.as_ref()
            .ok_or(ValidationError("password auth is disabled"))?;
        let password_correct = verify_password(
            password_hash,
            password,
        ).map_err(|_| HttpError::InternalError)?;
        if !password_correct {
            return Err(ValidationError("incorrect password").into());
        };
    };
    let access_token = generate_access_token();
    let created_at = Utc::now();
    let expires_at = created_at + Duration::seconds(ACCESS_TOKEN_EXPIRES_IN);
    save_oauth_token(
        db_client,
        &user.id,
        &access_token,
        &created_at,
        &expires_at,
    ).await?;
    log::warn!("created auth token for user {}", user.id);
    let token_response = TokenResponse::new(
        access_token,
        created_at.timestamp(),
    );
    Ok(HttpResponse::Ok().json(token_response))
}

#[post("/revoke")]
async fn revoke_token_view(
    auth: BearerAuth,
    db_pool: web::Data<DbPool>,
    request_data: web::Json<RevocationRequest>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    match delete_oauth_token(
        db_client,
        &current_user.id,
        &request_data.token,
    ).await {
        Ok(_) => (),
        Err(DatabaseError::NotFound(_)) => return Err(HttpError::PermissionError),
        Err(other_error) => return Err(other_error.into()),
    };
    Ok(HttpResponse::Ok().finish())
}

pub fn oauth_api_scope() -> ActixScope {
    web::scope("/oauth")
        .service(authorization_page_view)
        .service(authorize_view)
        .service(token_view)
        .service(revoke_token_view)
}
