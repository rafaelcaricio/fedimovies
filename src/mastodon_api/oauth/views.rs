use actix_web::{post, web, HttpResponse, Scope as ActixScope};
use chrono::{Duration, Utc};

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{HttpError, ValidationError};
use crate::ethereum::eip4361::verify_eip4361_signature;
use crate::models::oauth::queries::save_oauth_token;
use crate::models::users::queries::{
    get_user_by_name,
    get_user_by_wallet_address,
};
use crate::utils::crypto::verify_password;
use crate::utils::currencies::validate_wallet_address;
use super::types::{TokenRequest, TokenResponse};
use super::utils::generate_access_token;

const ACCESS_TOKEN_EXPIRES_IN: i64 = 86400 * 7;

/// OAuth 2.0 Password Grant
/// https://oauth.net/2/grant-types/password/
#[post("/token")]
async fn token_view(
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    request_data: web::Json<TokenRequest>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = match request_data.grant_type.as_str() {
        "password" => {
            let username = request_data.username.as_ref()
                .ok_or(ValidationError("username is required"))?;
            get_user_by_name(db_client, username).await?
        },
        "ethereum" => {
            // DEPRECATED
            let wallet_address = request_data.wallet_address.as_ref()
                .ok_or(ValidationError("wallet address is required"))?;
            validate_wallet_address(&config.default_currency(), wallet_address)?;
            get_user_by_wallet_address(db_client, wallet_address).await?
        },
        "eip4361" => {
            let message = request_data.message.as_ref()
                .ok_or(ValidationError("message is required"))?;
            let signature = request_data.signature.as_ref()
                .ok_or(ValidationError("signature is required"))?;
            let wallet_address = verify_eip4361_signature(
                message,
                signature,
                &config.instance().host(),
                &config.login_message,
            )?;
            get_user_by_wallet_address(db_client, &wallet_address).await?
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

pub fn oauth_api_scope() -> ActixScope {
    web::scope("/oauth")
        .service(token_view)
}
