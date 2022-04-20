use std::convert::TryInto;
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

impl ToString for SignatureData {
    fn to_string(&self) -> String {
        let mut bytes = Vec::with_capacity(65);
        bytes.extend_from_slice(&self.r);
        bytes.extend_from_slice(&self.s);
        let v: u8 = self.v.try_into()
            .expect("signature recovery in electrum notation always fits in a u8");
        bytes.push(v);
        hex::encode(bytes)
    }
}

impl FromStr for SignatureData {
    type Err = SignatureError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; 65];
        hex::decode_to_slice(value, &mut bytes)
            .map_err(|_| Self::Err::InvalidSignature)?;
        let v = bytes[64].into();
        let r = bytes[0..32].try_into()
            .map_err(|_| Self::Err::InvalidSignature)?;
        let s = bytes[32..64].try_into()
            .map_err(|_| Self::Err::InvalidSignature)?;
        let signature_data = Self { v, r, s };
        Ok(signature_data)
    }
}

fn prepare_message(message: &[u8]) -> [u8; 32] {
    let eip_191_message = [
        "\x19Ethereum Signed Message:\n".as_bytes(),
        message.len().to_string().as_bytes(),
        message,
    ].concat();
    let eip_191_message_hash = keccak256(&eip_191_message);
    eip_191_message_hash
}

/// Create EIP-191 signature
/// https://eips.ethereum.org/EIPS/eip-191
pub fn sign_message(
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
    fn test_signature_string_conversion() {
        let v = 28;
        let r: [u8; 32] = hex::decode("b91467e570a6466aa9e9876cbcd013baba02900b8979d43fe208a4a4f339f5fd")
            .unwrap().try_into().unwrap();
        let s: [u8; 32] = hex::decode("6007e74cd82e037b800186422fc2da167c747ef045e5d18a5f5d4300f8e1a029")
            .unwrap().try_into().unwrap();
        let expected_signature =
            "b91467e570a6466aa9e9876cbcd013baba02900b8979d43fe208a4a4f339f5fd6007e74cd82e037b800186422fc2da167c747ef045e5d18a5f5d4300f8e1a0291c";

        let signature_data = SignatureData { v, r, s };
        let signature_str = signature_data.to_string();
        assert_eq!(signature_str, expected_signature);

        let parsed = signature_str.parse::<SignatureData>().unwrap();
        assert_eq!(parsed.v, v);
        assert_eq!(parsed.r, r);
        assert_eq!(parsed.s, s);
    }

    #[test]
    fn test_signature_from_string_with_0x_prefix() {
        let signature_str = "0xb91467e570a6466aa9e9876cbcd013baba02900b8979d43fe208a4a4f339f5fd6007e74cd82e037b800186422fc2da167c747ef045e5d18a5f5d4300f8e1a0291c";
        let result = signature_str.parse::<SignatureData>();
        assert_eq!(result.is_err(), true);
    }

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
