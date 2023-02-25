use actix_web::{
    dev::ConnectionInfo,
    get,
    post,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_config::Config;
use mitra_utils::currencies::Currency;

use crate::activitypub::builders::update_person::prepare_update_person;
use crate::database::{get_database_client, DbPool};
use crate::errors::ValidationError;
use crate::ethereum::{
    contracts::ContractSet,
    subscriptions::{
        create_subscription_signature,
        is_registered_recipient,
    },
};
use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::types::Account,
    errors::MastodonError,
    oauth::auth::get_current_user,
};
use crate::models::{
    invoices::queries::{create_invoice, get_invoice_by_id},
    profiles::queries::{
        get_profile_by_id,
        update_profile,
    },
    profiles::types::{
        MoneroSubscription,
        PaymentOption,
        PaymentType,
        ProfileUpdateData,
    },
    subscriptions::queries::get_subscription_by_participants,
    users::queries::get_user_by_id,
    users::types::Permission,
};
use crate::monero::{
    helpers::validate_monero_address,
    wallet::create_monero_address,
};
use super::types::{
    Invoice,
    InvoiceData,
    SubscriptionAuthorizationQueryParams,
    SubscriptionDetails,
    SubscriptionOption,
    SubscriptionQueryParams,
};

#[get("/authorize")]
pub async fn authorize_subscription(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<SubscriptionAuthorizationQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let ethereum_config = config.blockchain()
        .ok_or(MastodonError::NotSupported)?
        .ethereum_config()
        .ok_or(MastodonError::NotSupported)?;
    // The user must have a public ethereum address,
    // because subscribers should be able
    // to verify that payments are actually sent to the recipient.
    let wallet_address = current_user
        .public_wallet_address(&Currency::Ethereum)
        .ok_or(MastodonError::PermissionError)?;
    let signature = create_subscription_signature(
        ethereum_config,
        &wallet_address,
        query_params.price,
    ).map_err(|_| MastodonError::InternalError)?;
    Ok(HttpResponse::Ok().json(signature))
}

#[get("/options")]
async fn get_subscription_options(
    auth: BearerAuth,
    db_pool: web::Data<DbPool>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    let options: Vec<SubscriptionOption> = current_user.profile
        .payment_options.into_inner().into_iter()
        .filter_map(SubscriptionOption::from_payment_option)
        .collect();
    Ok(HttpResponse::Ok().json(options))
}

#[post("/options")]
pub async fn register_subscription_option(
    auth: BearerAuth,
    connection_info: ConnectionInfo,
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    maybe_blockchain: web::Data<Option<ContractSet>>,
    subscription_option: web::Json<SubscriptionOption>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let mut current_user = get_current_user(db_client, auth.token()).await?;
    if !current_user.role.has_permission(Permission::ManageSubscriptionOptions) {
        return Err(MastodonError::PermissionError);
    };

    let maybe_payment_option = match subscription_option.into_inner() {
        SubscriptionOption::Ethereum => {
            let ethereum_config = config.blockchain()
                .and_then(|conf| conf.ethereum_config())
                .ok_or(MastodonError::NotSupported)?;
            let contract_set = maybe_blockchain.as_ref().as_ref()
                .ok_or(MastodonError::NotSupported)?;
            let wallet_address = current_user
                .public_wallet_address(&Currency::Ethereum)
                .ok_or(MastodonError::PermissionError)?;
            if current_user.profile.payment_options
                .any(PaymentType::EthereumSubscription)
            {
                // Ignore attempts to update payment option
                None
            } else {
                let is_registered = is_registered_recipient(
                    contract_set,
                    &wallet_address,
                ).await.map_err(|_| MastodonError::InternalError)?;
                if !is_registered {
                    return Err(ValidationError("recipient is not registered").into());
                };
                Some(PaymentOption::ethereum_subscription(
                    ethereum_config.chain_id.clone(),
                ))
            }
        },
        SubscriptionOption::Monero { price, payout_address } => {
            let monero_config = config.blockchain()
                .and_then(|conf| conf.monero_config())
                .ok_or(MastodonError::NotSupported)?;
            if price == 0 {
                return Err(ValidationError("price must be greater than 0").into());
            };
            validate_monero_address(&payout_address)?;
            let payment_info = MoneroSubscription {
                chain_id: monero_config.chain_id.clone(),
                price,
                payout_address,
            };
            Some(PaymentOption::MoneroSubscription(payment_info))
        },
    };
    if let Some(payment_option) = maybe_payment_option {
        let mut profile_data = ProfileUpdateData::from(&current_user.profile);
        profile_data.add_payment_option(payment_option);
        current_user.profile = update_profile(
            db_client,
            &current_user.id,
            profile_data,
        ).await?;

        // Federate
        prepare_update_person(
            db_client,
            &config.instance(),
            &current_user,
            None,
        ).await?.enqueue(db_client).await?;
    };

    let account = Account::from_user(
        &get_request_base_url(connection_info),
        &config.instance_url(),
        current_user,
    );
    Ok(HttpResponse::Ok().json(account))
}

#[get("/find")]
async fn find_subscription(
    db_pool: web::Data<DbPool>,
    query_params: web::Query<SubscriptionQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let subscription = get_subscription_by_participants(
        db_client,
        &query_params.sender_id,
        &query_params.recipient_id,
    ).await?;
    let details = SubscriptionDetails {
        id: subscription.id,
        expires_at: subscription.expires_at,
    };
    Ok(HttpResponse::Ok().json(details))
}

#[post("/invoices")]
async fn create_invoice_view(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    invoice_data: web::Json<InvoiceData>,
) -> Result<HttpResponse, MastodonError> {
    let monero_config = config.blockchain()
        .ok_or(MastodonError::NotSupported)?
        .monero_config()
        .ok_or(MastodonError::NotSupported)?;
    if invoice_data.sender_id == invoice_data.recipient_id {
        return Err(ValidationError("sender must be different from recipient").into());
    };
    if invoice_data.amount <= 0 {
        return Err(ValidationError("amount must be positive").into());
    };
    let db_client = &**get_database_client(&db_pool).await?;
    let sender = get_profile_by_id(db_client, &invoice_data.sender_id).await?;
    let recipient = get_user_by_id(db_client, &invoice_data.recipient_id).await?;
    if !recipient.profile.payment_options.any(PaymentType::MoneroSubscription) {
        let error_message = "recipient can't accept subscription payments";
        return Err(ValidationError(error_message).into());
    };

    let payment_address = create_monero_address(monero_config).await
        .map_err(|_| MastodonError::InternalError)?
        .to_string();
    let db_invoice = create_invoice(
        db_client,
        &sender.id,
        &recipient.id,
        &monero_config.chain_id,
        &payment_address,
        invoice_data.amount,
    ).await?;
    let invoice = Invoice::from(db_invoice);
    Ok(HttpResponse::Ok().json(invoice))
}

#[get("/invoices/{invoice_id}")]
async fn get_invoice(
    db_pool: web::Data<DbPool>,
    invoice_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let db_invoice = get_invoice_by_id(db_client, &invoice_id).await?;
    let invoice = Invoice::from(db_invoice);
    Ok(HttpResponse::Ok().json(invoice))
}

pub fn subscription_api_scope() -> Scope {
    web::scope("/api/v1/subscriptions")
        .service(authorize_subscription)
        .service(get_subscription_options)
        .service(register_subscription_option)
        .service(find_subscription)
        .service(create_invoice_view)
        .service(get_invoice)
}
