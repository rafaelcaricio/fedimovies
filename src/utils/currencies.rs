use regex::Regex;

use crate::errors::{ConversionError, ValidationError};
use super::caip2::ChainId;

#[derive(Debug, PartialEq)]
pub enum Currency {
    Ethereum,
    Monero,
}

impl Currency {
    fn code(&self) -> String {
        match self {
            Self::Ethereum => "ETH",
            Self::Monero => "XMR",
        }.to_string()
    }

    pub fn field_name(&self) -> String {
        format!("${}", self.code())
    }

    pub fn normalize_address(&self, address: &str) -> String {
        match self {
            Self::Ethereum => address.to_lowercase(),
            Self::Monero => address.to_string(),
        }
    }
}

impl TryFrom<&ChainId> for Currency {
    type Error = ConversionError;

    fn try_from(value: &ChainId) -> Result<Self, Self::Error> {
        let currency = match value.namespace.as_str() {
            "eip155" => Self::Ethereum,
            "monero" => Self::Monero, // not standard
            _ => return Err(ConversionError),
        };
        Ok(currency)
    }
}

pub fn validate_wallet_address(
    currency: &Currency,
    wallet_address: &str,
) -> Result<(), ValidationError> {
    match currency {
        Currency::Ethereum => {
            // Address should be lowercase
            let address_regexp = Regex::new(r"^0x[a-f0-9]{40}$").unwrap();
            if !address_regexp.is_match(wallet_address) {
                return Err(ValidationError("address is not lowercase"));
            };
        },
        Currency::Monero => (), // no validation
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_id_conversion() {
        let ethereum_chain_id = ChainId::ethereum_mainnet();
        let currency = Currency::try_from(&ethereum_chain_id).unwrap();
        assert_eq!(currency, Currency::Ethereum);

        let monero_chain_id = ChainId {
            namespace: "monero".to_string(),
            reference: "mainnet".to_string(),
        };
        let currency = Currency::try_from(&monero_chain_id).unwrap();
        assert_eq!(currency, Currency::Monero);
    }

    #[test]
    fn test_get_currency_field_name() {
        let ethereum = Currency::Ethereum;
        assert_eq!(ethereum.field_name(), "$ETH");
    }

    #[test]
    fn test_validate_wallet_address() {
        let ethereum = Currency::Ethereum;
        let result_1 = validate_wallet_address(&ethereum, "0xab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert_eq!(result_1.is_ok(), true);
        let result_2 = validate_wallet_address(&ethereum, "ab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert_eq!(result_2.is_ok(), false);
        let result_3 = validate_wallet_address(&ethereum, "0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B");
        assert_eq!(result_3.is_ok(), false);
    }
}
