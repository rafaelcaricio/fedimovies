use std::str::FromStr;

use monero_rpc::{RpcClient, TransferType};
use monero_rpc::monero::{Address, Amount};

use crate::config::MoneroConfig;
use crate::database::{get_database_client, Pool};
use crate::models::{
    invoices::queries::{
        get_invoice_by_address,
        get_invoices_by_status,
        set_invoice_status,
    },
    invoices::types::InvoiceStatus,
    profiles::types::PaymentOption,
    users::queries::get_user_by_id,
};
use super::wallet::{send_monero, DEFAULT_ACCOUNT, MoneroError};

pub async fn check_monero_subscriptions(
    config: &MoneroConfig,
    db_pool: &Pool,
) -> Result<(), MoneroError> {
    let db_client = &**get_database_client(db_pool).await?;

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
        let recipient = get_user_by_id(db_client, &invoice.recipient_id).await?;
        let maybe_payment_info = recipient.profile.payment_options
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
        let _payout_amount = send_monero(
            &wallet_client,
            address_index.minor,
            payout_address,
        ).await?;
        set_invoice_status(
            db_client,
            &invoice.id,
            InvoiceStatus::Forwarded,
        ).await?;
        log::info!("processed payment for invoice {}", invoice.id);
    };
    Ok(())
}
