use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::invoices::types::DbInvoice;
use crate::models::profiles::types::PaymentOption;

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
