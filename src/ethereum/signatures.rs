use std::str::FromStr;

use secp256k1::{Error as KeyError, SecretKey, rand::rngs::OsRng};
use serde::Serialize;
use web3::ethabi::{token::Token, encode};
use web3::signing::{keccak256, Key, SigningError};
use web3::types::{Address, U256};

/// Generates signing key
pub fn generate_ecdsa_key() -> SecretKey {
    let mut rng = OsRng::new().expect("failed to initialize RNG");
    SecretKey::new(&mut rng)
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

    #[error("invalid data")]
    InvalidData,

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

pub type CallArgs = Vec<Box<dyn AsRef<[u8]>>>;

pub fn sign_contract_call(
    signing_key: &str,
    chain_id: u32,
    contract_address: &str,
    method_name: &str,
    method_args: CallArgs,
) -> Result<SignatureData, SignatureError> {
    let chain_id: U256 = chain_id.into();
    let chain_id_token = Token::Uint(chain_id);
    let chain_id_bin = encode(&[chain_id_token]);
    let contract_address = Address::from_str(contract_address)
        .map_err(|_| SignatureError::InvalidData)?;
    let mut message = [
        &chain_id_bin,
        contract_address.as_bytes(),
        method_name.as_bytes(),
    ].concat();
    for arg in method_args {
        message.extend(arg.as_ref().as_ref());
    };
    let signature = sign_message(signing_key, &message)?;
    Ok(signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_message() {
        let signing_key = generate_ecdsa_key().to_string();
        let message = "test_message";
        let result = sign_message(&signing_key, message.as_bytes()).unwrap();
        assert!(result.v == 27 || result.v == 28);
    }

    #[test]
    fn test_sign_contract_call() {
        let signing_key = generate_ecdsa_key().to_string();
        let chain_id = 1;
        let contract_address = "0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0";
        let method_name = "test";
        let method_args: CallArgs = vec![Box::new("arg1"), Box::new("arg2")];
        let result = sign_contract_call(
            &signing_key,
            chain_id,
            contract_address,
            method_name,
            method_args,
        ).unwrap();
        assert!(result.v == 27 || result.v == 28);
    }
}
