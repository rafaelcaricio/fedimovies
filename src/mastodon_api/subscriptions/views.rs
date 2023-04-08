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
use mitra_models::{
    database::{get_database_client, DbPool},
    invoices::queries::{get_invoice_by_id},
    subscriptions::queries::get_subscription_by_participants,
    users::types::Permission,
};
use crate::http::get_request_base_url;
use crate::mastodon_api::{
    accounts::types::Account,
    errors::MastodonError,
    oauth::auth::get_current_user,
};

use super::types::{
    Invoice,
    SubscriptionAuthorizationQueryParams,
    SubscriptionDetails,
    SubscriptionOption,
    SubscriptionQueryParams,
};

#[get("/authorize")]
pub async fn authorize_subscription(
    auth: BearerAuth,
    _config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    _query_params: web::Query<SubscriptionAuthorizationQueryParams>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let _current_user = get_current_user(db_client, auth.token()).await?;

    // The user must have a public ethereum address,
    // because subscribers should be able
    // to verify that payments are actually sent to the recipient.
    return Err(MastodonError::PermissionError);
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
    _subscription_option: web::Json<SubscriptionOption>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !current_user.role.has_permission(Permission::ManageSubscriptionOptions) {
        return Err(MastodonError::PermissionError);
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
        .service(get_invoice)
}
