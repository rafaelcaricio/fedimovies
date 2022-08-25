use actix_web::{web, HttpResponse, Scope};
use actix_web_httpauth::extractors::bearer::BearerAuth;

use crate::activitypub::builders::update_person::prepare_update_person;
use crate::config::Config;
use crate::database::{Pool, get_database_client};
use crate::errors::{HttpError, ValidationError};
use crate::ethereum::contracts::ContractSet;
use crate::ethereum::subscriptions::{
    create_subscription_signature,
    is_registered_recipient,
};
use crate::mastodon_api::accounts::types::Account;
use crate::mastodon_api::oauth::auth::get_current_user;
use crate::models::profiles::queries::update_profile;
use crate::models::profiles::types::{
    PaymentOption,
    PaymentType,
    ProfileUpdateData,
};
use crate::utils::currencies::Currency;
use super::types::{
    SubscriptionQueryParams,
    SubscriptionSettings,
};

pub async fn authorize_subscription(
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

pub async fn subscriptions_enabled(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<Pool>,
    maybe_blockchain: web::Data<Option<ContractSet>>,
    subscription_settings: web::Json<SubscriptionSettings>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;

    let mut maybe_payment_option = None;
    match subscription_settings.into_inner() {
        SubscriptionSettings::Ethereum => {
            let contract_set = maybe_blockchain.as_ref().as_ref()
                .ok_or(HttpError::NotSupported)?;
            let wallet_address = current_user
                .public_wallet_address(&Currency::Ethereum)
                .ok_or(HttpError::PermissionError)?;
            if !current_user.profile.payment_options
                .any(PaymentType::EthereumSubscription)
            {
                let is_registered = is_registered_recipient(
                    contract_set,
                    &wallet_address,
                ).await.map_err(|_| HttpError::InternalError)?;
                if !is_registered {
                    return Err(ValidationError("recipient is not registered").into());
                };
                maybe_payment_option = Some(PaymentOption::EthereumSubscription);
            };
        },
        SubscriptionSettings::Monero { } => {
            todo!();
        },
    };
    if let Some(payment_option) = maybe_payment_option {
        // Add payment option to profile
        let mut profile_data = ProfileUpdateData::from(&current_user.profile);
        profile_data.payment_options.push(payment_option);
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

pub fn subscription_api_scope() -> Scope {
    web::scope("/api/v1/subscriptions")
        .route("/authorize", web::get().to(authorize_subscription))
        .route("/enable", web::post().to(subscriptions_enabled))
}
