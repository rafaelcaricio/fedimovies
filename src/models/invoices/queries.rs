use tokio_postgres::GenericClient;
use uuid::Uuid;

use crate::database::catch_unique_violation;
use crate::errors::DatabaseError;
use crate::utils::caip2::ChainId;
use crate::utils::id::new_uuid;
use super::types::{DbInvoice, InvoiceStatus};

pub async fn create_invoice(
    db_client: &impl GenericClient,
    sender_id: &Uuid,
    recipient_id: &Uuid,
    chain_id: &ChainId,
    payment_address: &str,
    amount: i64,
) -> Result<DbInvoice, DatabaseError> {
    let invoice_id = new_uuid();
    let row = db_client.query_one(
        "
        INSERT INTO invoice (
            id,
            sender_id,
            recipient_id,
            chain_id,
            payment_address,
            amount
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING invoice
        ",
        &[
            &invoice_id,
            &sender_id,
            &recipient_id,
            &chain_id,
            &payment_address,
            &amount,
        ],
    ).await.map_err(catch_unique_violation("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub async fn get_invoice_by_id(
    db_client: &impl GenericClient,
    invoice_id: &Uuid,
) -> Result<DbInvoice, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT invoice
        FROM invoice WHERE id = $1
        ",
        &[&invoice_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub async fn get_invoice_by_address(
    db_client: &impl GenericClient,
    chain_id: &ChainId,
    payment_address: &str,
) -> Result<DbInvoice, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT invoice
        FROM invoice WHERE chain_id = $1 AND payment_address = $2
        ",
        &[&chain_id, &payment_address],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("invoice"))?;
    let invoice = row.try_get("invoice")?;
    Ok(invoice)
}

pub async fn get_invoices_by_status(
    db_client: &impl GenericClient,
    chain_id: &ChainId,
    status: InvoiceStatus,
) -> Result<Vec<DbInvoice>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT invoice
        FROM invoice WHERE chain_id = $1 AND invoice_status = $2
        ",
        &[&chain_id, &status],
    ).await?;
    let invoices = rows.iter()
        .map(|row| row.try_get("invoice"))
        .collect::<Result<_, _>>()?;
    Ok(invoices)
}

pub async fn set_invoice_status(
    db_client: &impl GenericClient,
    invoice_id: &Uuid,
    status: InvoiceStatus,
) -> Result<(), DatabaseError> {
    let updated_count = db_client.execute(
        "
        UPDATE invoice SET invoice_status = $2
        WHERE id = $1
        ",
        &[&invoice_id, &status],
    ).await?;
    if updated_count == 0 {
        return Err(DatabaseError::NotFound("invoice"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::models::{
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
        let amount = 100000000000109212;
        let invoice = create_invoice(
            db_client,
            &sender.id,
            &recipient.id,
            &chain_id,
            payment_address,
            amount,
        ).await.unwrap();
        assert_eq!(invoice.sender_id, sender.id);
        assert_eq!(invoice.recipient_id, recipient.id);
        assert_eq!(invoice.chain_id, chain_id);
        assert_eq!(invoice.payment_address, payment_address);
        assert_eq!(invoice.amount, amount);
        assert_eq!(invoice.invoice_status, InvoiceStatus::Open);
    }
}
