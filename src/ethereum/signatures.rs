use std::str::FromStr;

use secp256k1::{Error as KeyError, SecretKey, rand::rngs::OsRng};
use serde::Serialize;
use web3::ethabi::{token::Token, encode};
use web3::signing::{
    keccak256,
    recover,
    Key,
    RecoveryError,
    SecretKeyRef,
    SigningError,
};
use web3::types::{Address, H256, Recovery, U256};

/// Generates signing key
pub fn generate_ecdsa_key() -> SecretKey {
    let mut rng = OsRng::new().expect("failed to initialize RNG");
    SecretKey::new(&mut rng)
}

#[derive(Serialize)]
pub struct SignatureData {
    pub v: u64,
    #[serde(serialize_with = "hex::serde::serialize")]
    pub r: [u8; 32],
    #[serde(serialize_with = "hex::serde::serialize")]
    pub s: [u8; 32],
}

#[derive(thiserror::Error, Debug)]
pub enum SignatureError {
    #[error("invalid key")]
    InvalidKey(#[from] KeyError),

    #[error("invalid data")]
    InvalidData,

    #[error("signing error")]
    SigningError(#[from] SigningError),

    #[error("invalid signature")]
    InvalidSignature,

    #[error("recovery error")]
    RecoveryError(#[from] RecoveryError),
}

fn prepare_message(message: &[u8]) -> [u8; 32] {
    let eip_191_message = [
        "\x19Ethereum Signed Message:\n".as_bytes(),
        message.len().to_string().as_bytes(),
        &message,
    ].concat();
    let eip_191_message_hash = keccak256(&eip_191_message);
    eip_191_message_hash
}

/// Create EIP-191 signature
/// https://eips.ethereum.org/EIPS/eip-191
fn sign_message(
    signing_key: &str,
    message: &[u8],
) -> Result<SignatureData, SignatureError> {
    let key = SecretKey::from_str(signing_key)?;
    let key_ref = SecretKeyRef::new(&key);
    let eip_191_message_hash = prepare_message(message);
    // Create signature without replay protection (chain ID is None)
    let signature = key_ref.sign(&eip_191_message_hash, None)?;
    let signature_data = SignatureData {
        v: signature.v,
        r: signature.r.to_fixed_bytes(),
        s: signature.s.to_fixed_bytes(),
    };
    Ok(signature_data)
}

/// Verify EIP-191 signature
pub fn recover_address(
    message: &[u8],
    signature: &SignatureData,
) -> Result<Address, SignatureError> {
    let eip_191_message_hash = prepare_message(message);
    let recovery = Recovery::new(
        "", // this message is not used
        signature.v,
        H256(signature.r),
        H256(signature.s),
    );
    let (signature_raw, recovery_id) = recovery.as_signature()
        .ok_or(SignatureError::InvalidSignature)?;
    let address = recover(
        &eip_191_message_hash,
        &signature_raw,
        recovery_id,
    )?;
    Ok(address)
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
    let message_hash = keccak256(&message);
    let signature = sign_message(signing_key, &message_hash)?;
    Ok(signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_message() {
        let signing_key = generate_ecdsa_key();
        let message = "test_message";
        let result = sign_message(
            &signing_key.display_secret().to_string(),
            message.as_bytes(),
        ).unwrap();
        assert!(result.v == 27 || result.v == 28);

        let recovered = recover_address(message.as_bytes(), &result).unwrap();
        assert_eq!(recovered, SecretKeyRef::new(&signing_key).address());
    }

    #[test]
    fn test_sign_contract_call() {
        let signing_key = generate_ecdsa_key().display_secret().to_string();
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
