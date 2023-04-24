use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_models::{
    invoices::types::{DbInvoice, InvoiceStatus},
    profiles::types::PaymentOption,
};

#[derive(Deserialize)]
pub struct InvoiceData {
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub amount: i64,
}

#[derive(Serialize)]
pub struct Invoice {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub payment_address: String,
    pub amount: i64,
    pub status: String,
    pub expires_at: DateTime<Utc>,
}

impl From<DbInvoice> for Invoice {
    fn from(value: DbInvoice) -> Self {
        let status = match value.invoice_status {
            InvoiceStatus::Open => "open",
            InvoiceStatus::Paid => "paid",
            InvoiceStatus::Forwarded => "forwarded",
            InvoiceStatus::Timeout => "timeout",
        };
        let expires_at = Default::default();
        Self {
            id: value.id,
            sender_id: value.sender_id,
            recipient_id: value.recipient_id,
            payment_address: value.payment_address,
            amount: value.amount,
            status: status.to_string(),
            expires_at,
        }
    }
}

#[derive(Deserialize)]
pub struct SubscriptionAuthorizationQueryParams {
    pub price: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SubscriptionOption {
    Ethereum,
    Monero { price: u64, payout_address: String },
}

impl SubscriptionOption {
    pub fn from_payment_option(payment_option: PaymentOption) -> Option<Self> {
        let settings = match payment_option {
            PaymentOption::Link(_) => return None,
            PaymentOption::EthereumSubscription(_) => Self::Ethereum,
            PaymentOption::MoneroSubscription(payment_info) => Self::Monero {
                price: payment_info.price,
                payout_address: payment_info.payout_address,
            },
        };
        Some(settings)
    }
}

#[derive(Deserialize)]
pub struct SubscriptionQueryParams {
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
}

#[derive(Serialize)]
pub struct SubscriptionDetails {
    pub id: i32,
    pub expires_at: DateTime<Utc>,
}
