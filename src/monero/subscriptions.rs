use std::str::FromStr;

use chrono::{Duration, Utc};
use monero_rpc::TransferType;
use monero_rpc::monero::{Address, Amount};

use crate::config::{Instance, MoneroConfig};
use crate::database::{get_database_client, DatabaseError, DbPool};
use crate::ethereum::subscriptions::send_subscription_notifications;
use crate::models::{
    invoices::queries::{
        get_invoice_by_address,
        get_invoices_by_status,
        set_invoice_status,
    },
    invoices::types::InvoiceStatus,
    profiles::queries::get_profile_by_id,
    profiles::types::PaymentOption,
    subscriptions::queries::{
        create_subscription,
        get_subscription_by_participants,
        update_subscription,
    },
    users::queries::get_user_by_id,
};
use super::wallet::{
    get_single_item,
    get_subaddress_balance,
    open_monero_wallet,
    send_monero,
    DEFAULT_ACCOUNT,
    MoneroError,
};

pub const MONERO_INVOICE_TIMEOUT: i64 = 3 * 60 * 60; // 3 hours

pub async fn check_monero_subscriptions(
    instance: &Instance,
    config: &MoneroConfig,
    db_pool: &DbPool,
) -> Result<(), MoneroError> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let wallet_client = open_monero_wallet(config).await?;

    // Invoices waiting for payment
    let mut address_waitlist = vec![];
    let open_invoices = get_invoices_by_status(
        db_client,
        &config.chain_id,
        InvoiceStatus::Open,
    ).await?;
    for invoice in open_invoices {
        let invoice_age = Utc::now() - invoice.created_at;
        if invoice_age.num_seconds() >= MONERO_INVOICE_TIMEOUT {
            set_invoice_status(
                db_client,
                &invoice.id,
                InvoiceStatus::Timeout,
            ).await?;
            continue;
        };
        let address = Address::from_str(&invoice.payment_address)?;
        let address_index = wallet_client.get_address_index(address).await?;
        address_waitlist.push(address_index.minor);
    };
    let maybe_incoming_transfers = if !address_waitlist.is_empty() {
        log::info!("{} invoices are waiting for payment", address_waitlist.len());
        let incoming_transfers = wallet_client.incoming_transfers(
            TransferType::Available,
            Some(DEFAULT_ACCOUNT),
            Some(address_waitlist),
        ).await?;
        incoming_transfers.transfers
    } else {
        None
    };
    if let Some(transfers) = maybe_incoming_transfers {
        for transfer in transfers {
            let address_data = wallet_client.get_address(
                DEFAULT_ACCOUNT,
                Some(vec![transfer.subaddr_index.minor]),
            ).await?;
            let subaddress_data = get_single_item(address_data.addresses)?;
            let subaddress = subaddress_data.address;
            let invoice = get_invoice_by_address(
                db_client,
                &config.chain_id,
                &subaddress.to_string(),
            ).await?;
            log::info!(
                "received payment for invoice {}: {}",
                invoice.id,
                transfer.amount,
            );
            if invoice.invoice_status == InvoiceStatus::Open {
                set_invoice_status(db_client, &invoice.id, InvoiceStatus::Paid).await?;
            } else {
                log::warn!("invoice has already been paid");
            };
        };
    };

    // Invoices waiting to be forwarded
    let paid_invoices = get_invoices_by_status(
        db_client,
        &config.chain_id,
        InvoiceStatus::Paid,
    ).await?;
    for invoice in paid_invoices {
        let address = Address::from_str(&invoice.payment_address)?;
        let address_index = wallet_client.get_address_index(address).await?;
        let balance_data = get_subaddress_balance(
            &wallet_client,
            &address_index,
        ).await?;
        if balance_data.balance != balance_data.unlocked_balance ||
            balance_data.balance == Amount::ZERO
        {
            // Don't forward payment until all outputs are unlocked
            continue;
        };
        let sender = get_profile_by_id(db_client, &invoice.sender_id).await?;
        let recipient = get_user_by_id(db_client, &invoice.recipient_id).await?;
        let maybe_payment_info = recipient.profile.payment_options.clone()
            .into_inner().into_iter()
            .find_map(|option| match option {
                PaymentOption::MoneroSubscription(payment_info) => {
                    if payment_info.chain_id == config.chain_id {
                        Some(payment_info)
                    } else {
                        None
                    }
                },
                _ => None,
            });
        let payment_info = if let Some(payment_info) = maybe_payment_info {
            payment_info
        } else {
            log::error!("subscription is not configured for user {}", recipient.id);
            continue;
        };
        let payout_address = Address::from_str(&payment_info.payout_address)?;
        let payout_amount = send_monero(
            &wallet_client,
            address_index.minor,
            payout_address,
        ).await?;
        let duration_secs = (payout_amount.as_pico() / payment_info.price)
            .try_into()
            .map_err(|_| MoneroError::OtherError("invalid duration"))?;

        set_invoice_status(
            db_client,
            &invoice.id,
            InvoiceStatus::Forwarded,
        ).await?;
        log::info!("processed payment for invoice {}", invoice.id);

        match get_subscription_by_participants(
            db_client,
            &sender.id,
            &recipient.id,
        ).await {
            Ok(subscription) => {
                if subscription.chain_id != config.chain_id {
                    log::error!("can't switch to another chain");
                    continue;
                };
                // Update subscription expiration date
                let expires_at =
                    std::cmp::max(subscription.expires_at, Utc::now()) +
                    Duration::seconds(duration_secs);
                update_subscription(
                    db_client,
                    subscription.id,
                    &subscription.chain_id,
                    &expires_at,
                    &Utc::now(),
                ).await?;
                log::info!(
                    "subscription updated: {0} to {1}",
                    subscription.sender_id,
                    subscription.recipient_id,
                );
                send_subscription_notifications(
                    db_client,
                    instance,
                    &sender,
                    &recipient,
                ).await?;
            },
            Err(DatabaseError::NotFound(_)) => {
                // New subscription
                let expires_at = Utc::now() + Duration::seconds(duration_secs);
                create_subscription(
                    db_client,
                    &sender.id,
                    None, // matching by address is not required
                    &recipient.id,
                    &config.chain_id,
                    &expires_at,
                    &Utc::now(),
                ).await?;
                log::info!(
                    "subscription created: {0} to {1}",
                    sender.id,
                    recipient.id,
                );
                send_subscription_notifications(
                    db_client,
                    instance,
                    &sender,
                    &recipient,
                ).await?;
            },
            Err(other_error) => return Err(other_error.into()),
        };
    };
    Ok(())
}
