use std::str::FromStr;

use monero_rpc::TransferType;
use monero_rpc::monero::Address;
use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::config::MoneroConfig;
use crate::models::{
    invoices::queries::{
        get_invoice_by_id,
        set_invoice_status,
    },
    invoices::types::InvoiceStatus,
};
use super::wallet::{
    open_monero_wallet,
    DEFAULT_ACCOUNT,
    MoneroError,
};

pub async fn check_expired_invoice(
    config: &MoneroConfig,
    db_client: &impl GenericClient,
    invoice_id: &Uuid,
) -> Result<(), MoneroError> {
    let wallet_client = open_monero_wallet(config).await?;
    let invoice = get_invoice_by_id(db_client, invoice_id).await?;
    if invoice.chain_id != config.chain_id ||
        invoice.invoice_status != InvoiceStatus::Timeout
    {
        return Err(MoneroError::OtherError("can't process invoice"));
    };
    let address = Address::from_str(&invoice.payment_address)?;
    let address_index = wallet_client.get_address_index(address).await?;
    let transfers = wallet_client.incoming_transfers(
        TransferType::Available,
        Some(DEFAULT_ACCOUNT),
        Some(vec![address_index.minor]),
    ).await?
        .transfers
        .unwrap_or_default();
    if transfers.is_empty() {
        log::info!("no incoming transfers");
    } else {
        for transfer in transfers {
            if transfer.subaddr_index != address_index {
                return Err(MoneroError::WalletRpcError("unexpected transfer"));
            };
            log::info!(
                "received payment for invoice {}: {}",
                invoice.id,
                transfer.amount,
            );
        };
        set_invoice_status(db_client, &invoice.id, InvoiceStatus::Paid).await?;
    };
    Ok(())
}
