use actix_web::{post, web, HttpResponse, Scope as ActixScope};
use chrono::{Duration, Utc};

use crate::database::{Pool, get_database_client};
use crate::errors::{HttpError, ValidationError};
use crate::models::oauth::queries::save_oauth_token;
use crate::models::users::queries::{
    get_user_by_name,
    get_user_by_wallet_address,
};
use crate::models::users::types::validate_wallet_address;
use crate::utils::crypto::verify_password;
use super::types::{TokenRequest, TokenResponse};
use super::utils::generate_access_token;

const ACCESS_TOKEN_EXPIRES_IN: i64 = 86400 * 7;

/// OAuth 2.0 Password Grant
/// https://oauth.net/2/grant-types/password/
#[post("/token")]
async fn token_view(
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
            let wallet_address = request_data.wallet_address.as_ref()
                .ok_or(ValidationError("wallet address is required"))?;
            validate_wallet_address(wallet_address)?;
            get_user_by_wallet_address(db_client, wallet_address).await?
        },
        _ => {
            return Err(ValidationError("unsupported grant type").into());
        },
    };
    let password_correct = verify_password(
        &user.password_hash,
        &request_data.password,
    ).map_err(|_| HttpError::InternalError)?;
    if !password_correct {
        // Invalid signature/password
        return Err(ValidationError("incorrect password").into());
    }
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
