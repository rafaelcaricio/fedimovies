use std::str::FromStr;

use secp256k1::SecretKey;
use web3::{
    signing::Key,
    types::Address,
};

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
