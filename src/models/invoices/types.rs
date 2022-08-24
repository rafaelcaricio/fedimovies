use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

use crate::database::int_enum::{int_enum_from_sql, int_enum_to_sql};
use crate::errors::ConversionError;
use crate::utils::caip2::ChainId;

#[derive(Debug)]
pub enum InvoiceStatus {
    Open,
    Paid,
    Forwarded,
}

impl From<&InvoiceStatus> for i16 {
    fn from(value: &InvoiceStatus) -> i16 {
        match value {
            InvoiceStatus::Open => 1,
            InvoiceStatus::Paid => 2,
            InvoiceStatus::Forwarded => 3,
        }
    }
}

impl TryFrom<i16> for InvoiceStatus {
    type Error = ConversionError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let invoice_status = match value {
            1 => Self::Open,
            2 => Self::Paid,
            3 => Self::Forwarded,
            _ => return Err(ConversionError),
        };
        Ok(invoice_status)
    }
}

int_enum_from_sql!(InvoiceStatus);
int_enum_to_sql!(InvoiceStatus);

#[derive(FromSql)]
#[postgres(name = "invoice")]
pub struct DbInvoice {
    pub id: Uuid,
    pub sender_id: Uuid,
    pub recipient_id: Uuid,
    pub chain_id: ChainId,
    pub payment_address: String,
    pub invoice_status: InvoiceStatus,
    pub created_at: DateTime<Utc>,
}
