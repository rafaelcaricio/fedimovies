use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::invoices::types::DbInvoice;

#[derive(Deserialize)]
pub struct InvoiceData {
    pub sender: String, // acct
    pub recipient: Uuid,
}

#[derive(Serialize)]
pub struct Invoice {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub payment_address: String,
}

impl From<DbInvoice> for Invoice {
    fn from(value: DbInvoice) -> Self {
        Self {
            id: value.id,
            sender_id: value.sender_id,
            recipient_id: value.recipient_id,
            payment_address: value.payment_address,
        }
    }
}

#[derive(Deserialize)]
pub struct SubscriptionQueryParams {
    pub price: u64,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum SubscriptionSettings {
    #[serde(rename = "ethereum")]
    Ethereum,
    #[serde(rename = "monero")]
    Monero { price: u64, payout_address: String },
}
