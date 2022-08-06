use std::convert::TryFrom;

use crate::errors::ConversionError;
use super::caip2::ChainId;

#[derive(Debug, PartialEq)]
pub enum Currency {
    Ethereum,
}

impl Currency {
    fn code(&self) -> String {
        match self {
            Self::Ethereum => "ETH",
        }.to_string()
    }

    pub fn field_name(&self) -> String {
        format!("${}", self.code())
    }

    /// Returns CAIP-2 chain ID
    pub fn chain_id(&self) -> ChainId {
        self.into()
    }

    pub fn normalize_address(&self, address: &str) -> String {
        match self {
            Self::Ethereum => address.to_lowercase(),
        }
    }
}

impl From<&Currency> for ChainId {
    fn from(value: &Currency) -> Self {
        let (namespace, reference) = match value {
            Currency::Ethereum => ("eip155", "1"),
        };
        Self {
            namespace: namespace.to_string(),
            reference: reference.to_string(),
        }
    }
}

impl TryFrom<&ChainId> for Currency {
    type Error = ConversionError;

    fn try_from(value: &ChainId) -> Result<Self, Self::Error> {
        let currency = match value.namespace.as_str() {
            "eip155" => match value.reference.as_str() {
                "1" => Self::Ethereum,
                _ => return Err(ConversionError),
            },
            _ => return Err(ConversionError),
        };
        Ok(currency)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_id_conversion() {
        let ethereum = Currency::Ethereum;
        let ethereum_chain_id = ChainId::from(&ethereum);
        let currency = Currency::try_from(&ethereum_chain_id).unwrap();
        assert_eq!(currency, ethereum);
    }

    #[test]
    fn test_get_currency_field_name() {
        let ethereum = Currency::Ethereum;
        assert_eq!(ethereum.field_name(), "$ETH");
    }
}