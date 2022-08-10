use std::str::FromStr;

use secp256k1::SecretKey;
use web3::{
    signing::Key,
    types::Address,
};

use crate::utils::caip2::ChainId;

#[derive(thiserror::Error, Debug)]
pub enum ChainIdError {
    #[error("unsupported chain")]
    UnsupportedChain,

    #[error("invalid EIP155 chain ID")]
    InvalidEip155ChainId(#[from] std::num::ParseIntError),
}

/// Parses CAIP-2 chain ID
pub fn parse_caip2_chain_id(chain_id: &ChainId) -> Result<u32, ChainIdError> {
    if chain_id.namespace != "eip155" {
        return Err(ChainIdError::UnsupportedChain);
    };
    let eth_chain_id: u32 = chain_id.reference.parse()?;
    Ok(eth_chain_id)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_caip2_chain_id() {
        let chain_id: ChainId = "eip155:1".parse().unwrap();
        let result = parse_caip2_chain_id(&chain_id).unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_parse_caip2_chain_id_unsupported() {
        let chain_id: ChainId = "bip122:000000000019d6689c085ae165831e93".parse().unwrap();
        let error = parse_caip2_chain_id(&chain_id).err().unwrap();
        assert!(matches!(error, ChainIdError::UnsupportedChain));
    }
}
