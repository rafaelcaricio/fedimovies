use actix_web::{
    post, web,
    HttpResponse,
};

use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{HttpError, ValidationError};
use crate::mastodon_api::accounts::types::{Account, AccountCreateData};
use crate::models::users::queries::{
    is_valid_invite_code,
    create_user,
};
use crate::utils::crypto::{
    hash_password,
    generate_private_key,
    serialize_private_key,
};

// /api/v1/accounts
#[post("")]
pub async fn create_user_view(
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
        if !is_valid_invite_code(db_client, &invite_code).await? {
            Err(ValidationError("invalid invite code"))?;
        }
    }
    // Hash password and generate private key
    let password_hash = hash_password(&user_data.password)
        .map_err(|_| HttpError::InternalError)?;
    let private_key = match web::block(move || generate_private_key()).await {
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

