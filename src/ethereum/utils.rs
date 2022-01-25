use std::str::FromStr;

use regex::Regex;
use secp256k1::{Error as KeyError, SecretKey, rand::rngs::OsRng};
use serde::Serialize;
use web3::{
    signing::{keccak256, Key, SigningError},
    types::Address,
};

#[derive(thiserror::Error, Debug)]
pub enum ChainIdError {
    #[error("invalid chain ID")]
    InvalidChainId,

    #[error("unsupported chain")]
    UnsupportedChain,

    #[error("invalid EIP155 chain ID")]
    InvalidEip155ChainId(#[from] std::num::ParseIntError),
}

/// Parses CAIP-2 chain ID
/// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-2.md
pub fn parse_caip2_chain_id(chain_id: &str) -> Result<u32, ChainIdError> {
    // eip155 namespace: ethereum chain
    let caip2_re = Regex::new(r"(?P<namespace>\w+):(?P<chain_id>\w+)").unwrap();
    let caip2_caps = caip2_re.captures(chain_id)
        .ok_or(ChainIdError::InvalidChainId)?;
    if &caip2_caps["namespace"] != "eip155" {
        return Err(ChainIdError::UnsupportedChain);
    };
    let eth_chain_id: u32 = caip2_caps["chain_id"].parse()?;
    Ok(eth_chain_id)
}

pub fn generate_ethereum_address() -> (SecretKey, Address) {
    let mut rng = OsRng::new().expect("failed to initialize RNG");
    let secret_key = SecretKey::new(&mut rng);
    let address = Box::new(secret_key).address();
    (secret_key, address)
}

#[derive(thiserror::Error, Debug)]
#[error("address error")]
pub struct AddressError;

pub fn parse_address(address: &str) -> Result<Address, AddressError> {
    Address::from_str(address).map_err(|_| AddressError)
}

#[derive(Serialize)]
pub struct SignatureData {
    pub v: u64,
    pub r: String,
    pub s: String,
}

#[derive(thiserror::Error, Debug)]
pub enum SignatureError {
    #[error("invalid key")]
    InvalidKey(#[from] KeyError),

    #[error("signing error")]
    SigningError(#[from] SigningError),
}

pub fn sign_message(
    signing_key: &str,
    message: &[u8],
) -> Result<SignatureData, SignatureError> {
    let key = SecretKey::from_str(signing_key)?;
    let message_hash = keccak256(message);
    let eip_191_message = [
        "\x19Ethereum Signed Message:\n32".as_bytes(),
        &message_hash,
    ].concat();
    let eip_191_message_hash = keccak256(&eip_191_message);
    let signature = Box::new(key).sign(&eip_191_message_hash, None)?;
    let signature_data = SignatureData {
        v: signature.v,
        r: hex::encode(signature.r.as_bytes()),
        s: hex::encode(signature.s.as_bytes()),
    };
    Ok(signature_data)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_caip2_chain_id() {
        let chain_id = "eip155:1";
        let result = parse_caip2_chain_id(chain_id).unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_parse_caip2_chain_id_unsupported() {
        let chain_id = "bip122:000000000019d6689c085ae165831e93";
        let error = parse_caip2_chain_id(chain_id).err().unwrap();
        assert!(matches!(error, ChainIdError::UnsupportedChain));
    }
}
