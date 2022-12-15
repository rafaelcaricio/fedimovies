use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use uuid::Uuid;

use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    DatabaseTypeError,
};
use crate::utils::caip2::ChainId;

#[derive(Debug, PartialEq)]
pub enum InvoiceStatus {
    Open,
    Paid,
    Forwarded,
    Timeout,
}

impl From<&InvoiceStatus> for i16 {
    fn from(value: &InvoiceStatus) -> i16 {
        match value {
            InvoiceStatus::Open => 1,
            InvoiceStatus::Paid => 2,
            InvoiceStatus::Forwarded => 3,
            InvoiceStatus::Timeout => 4,
        }
    }
}

impl TryFrom<i16> for InvoiceStatus {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let invoice_status = match value {
            1 => Self::Open,
            2 => Self::Paid,
            3 => Self::Forwarded,
            4 => Self::Timeout,
            _ => return Err(DatabaseTypeError),
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
    pub amount: i64, // requested payment amount
    pub invoice_status: InvoiceStatus,
    pub created_at: DateTime<Utc>,
}
