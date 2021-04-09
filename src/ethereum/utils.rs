use std::str::FromStr;

use secp256k1::{Error as KeyError, SecretKey, rand::rngs::OsRng};
use serde::Serialize;
use web3::{
    signing::{keccak256, Key, SigningError},
    types::Address,
};

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
    let key = SecretKey::from_str(&signing_key)?;
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
