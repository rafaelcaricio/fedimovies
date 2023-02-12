use std::str::FromStr;

use regex::Regex;
use secp256k1::SecretKey;
use web3::{
    signing::Key,
    types::Address,
};

use crate::errors::ValidationError;

pub fn key_to_ethereum_address(private_key: &SecretKey) -> Address {
    private_key.address()
}

#[derive(thiserror::Error, Debug)]
#[error("address error")]
pub struct AddressError;

pub fn parse_address(address: &str) -> Result<Address, AddressError> {
    Address::from_str(address).map_err(|_| AddressError)
}

/// Converts address object to lowercase hex string
pub fn address_to_string(address: Address) -> String {
    format!("{:#x}", address)
}

pub fn validate_ethereum_address(
    wallet_address: &str,
) -> Result<(), ValidationError> {
    let address_regexp = Regex::new(r"^0x[a-fA-F0-9]{40}$").unwrap();
    if !address_regexp.is_match(wallet_address) {
        return Err(ValidationError("invalid address"));
    };
    // Address should be lowercase
    if wallet_address.to_lowercase() != wallet_address {
        return Err(ValidationError("address is not lowercase"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ethereum_address() {
        let result_1 = validate_ethereum_address("0xab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert_eq!(result_1.is_ok(), true);
        let result_2 = validate_ethereum_address("ab5801a7d398351b8be11c439e05c5b3259aec9b");
        assert_eq!(
            result_2.err().unwrap().to_string(),
            "invalid address",
        );
        let result_3 = validate_ethereum_address("0xAb5801a7D398351b8bE11C439e05C5B3259aeC9B");
        assert_eq!(
            result_3.err().unwrap().to_string(),
            "address is not lowercase",
        );
    }
}
