use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::utils::caip2::ChainId;
use crate::utils::id::new_uuid;
use super::types::DbInvoice;

pub async fn create_invoice(
    db_client: &impl GenericClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    chain_id: &ChainId,
    payment_address: &str,
) -> Result<DbInvoice, DatabaseError> {
    let invoice_id = new_uuid();
    let row = db_client.query_one(
        "
        INSERT INTO invoice (
            id,
            sender_id,
            recipient_id,
            chain_id,
            payment_address
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING invoice
        ",
        &[
            &invoice_id,
            &sender_id,
            &recipient_id,
            &chain_id,
            &payment_address,
        ],
    ).await.map_err(catch_unique_violation("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::models::{
        invoices::types::InvoiceStatus,
        profiles::queries::create_profile,
        profiles::types::ProfileCreateData,
        users::queries::create_user,
        users::types::UserCreateData,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_invoice() {
        let db_client = &mut create_test_database().await;
        let sender_data = ProfileCreateData {
            username: "sender".to_string(),
            ..Default::default()
        };
        let sender = create_profile(db_client, sender_data).await.unwrap();
        let recipient_data = UserCreateData {
            username: "recipient".to_string(),
            ..Default::default()
        };
        let recipient = create_user(db_client, recipient_data).await.unwrap();
        let chain_id = ChainId {
            namespace: "monero".to_string(),
            reference: "mainnet".to_string(),
        };
        let payment_address = "8MxABajuo71BZya9";
        let invoice = create_invoice(
            db_client,
            &sender.id,
            &recipient.id,
            &chain_id,
            payment_address,
        ).await.unwrap();
        assert_eq!(invoice.sender_id, sender.id);
        assert_eq!(invoice.recipient_id, recipient.id);
        assert_eq!(invoice.chain_id, chain_id);
        assert_eq!(invoice.payment_address, payment_address);
        assert!(matches!(invoice.invoice_status, InvoiceStatus::Open));
    }
}
