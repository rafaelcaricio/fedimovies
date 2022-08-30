use std::convert::TryInto;
use std::str::FromStr;

use chrono::{Duration, Utc};
use monero_rpc::{RpcClient, TransferType};
use monero_rpc::monero::{Address, Amount};

use crate::config::{Instance, MoneroConfig};
use crate::database::{get_database_client, Pool};
use crate::errors::DatabaseError;
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
use super::wallet::{send_monero, DEFAULT_ACCOUNT, MoneroError};

pub async fn check_monero_subscriptions(
    instance: &Instance,
    config: &MoneroConfig,
    db_pool: &Pool,
) -> Result<(), MoneroError> {
    let db_client = &mut **get_database_client(db_pool).await?;

    let wallet_client = RpcClient::new(config.wallet_url.clone()).wallet();
    wallet_client.open_wallet(
        config.wallet_name.clone(),
        config.wallet_password.clone(),
    ).await?;

    // Invoices waiting for payment
    let mut address_waitlist = vec![];
    let open_invoices = get_invoices_by_status(
        db_client,
        &config.chain_id,
        InvoiceStatus::Open,
    ).await?;
    for invoice in open_invoices {
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
            let subaddress = if let [subaddress_data] = &address_data.addresses[..] {
                subaddress_data.address
            } else {
                return Err(MoneroError::OtherError("invalid response from wallet"));
            };
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
            set_invoice_status(db_client, &invoice.id, InvoiceStatus::Paid).await?;
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
        let balance_data = wallet_client.get_balance(
            address_index.major,
            Some(vec![address_index.minor]),
        ).await?;
        let unlocked_balance = if let [subaddress_data] = &balance_data.per_subaddress[..] {
            subaddress_data.unlocked_balance
        } else {
            return Err(MoneroError::OtherError("invalid response from wallet"));
        };
        if unlocked_balance == Amount::ZERO {
            // Not ready for forwarding
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
        let expires_at = Utc::now() + Duration::seconds(duration_secs);

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