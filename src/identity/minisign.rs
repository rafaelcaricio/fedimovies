/// https://jedisct1.github.io/minisign/
use blake2::{Blake2b512, Digest};
use ed25519_dalek::{
    PublicKey,
    Signature,
    SignatureError,
    Verifier,
};

use mitra_utils::did_key::{DidKey, MulticodecError};

const MINISIGN_SIGNATURE_CODE: [u8; 2] = *b"Ed";
const MINISIGN_SIGNATURE_HASHED_CODE: [u8; 2] = *b"ED";

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error("invalid encoding")]
    InvalidEncoding(#[from] base64::DecodeError),

    #[error("invalid key length")]
    InvalidKeyLength,

    #[error("invalid signature length")]
    InvalidSignatureLength,

    #[error("invalid signature type")]
    InvalidSignatureType,
}

// Public key format:
// base64(<signature_algorithm> || <key_id> || <public_key>)
fn parse_minisign_public_key(key_b64: &str)
    -> Result<[u8; 32], ParseError>
{
    let key_bin = base64::decode(key_b64)?;
    if key_bin.len() != 42 {
        return Err(ParseError::InvalidKeyLength);
    };

    let mut signature_algorithm = [0; 2];
    let mut _key_id = [0; 8];
    let mut key = [0; 32];
    signature_algorithm.copy_from_slice(&key_bin[0..2]);
    _key_id.copy_from_slice(&key_bin[2..10]);
    key.copy_from_slice(&key_bin[10..42]);

    if signature_algorithm.as_ref() != MINISIGN_SIGNATURE_CODE {
        return Err(ParseError::InvalidSignatureType);
    };
    Ok(key)
}

pub fn minisign_key_to_did(key_b64: &str) -> Result<DidKey, ParseError> {
    let key = parse_minisign_public_key(key_b64)?;
    let did_key = DidKey::from_ed25519_key(key);
    Ok(did_key)
}

// Signature format:
// base64(<signature_algorithm> || <key_id> || <signature>)
pub fn parse_minisign_signature(signature_b64: &str)
    -> Result<[u8; 64], ParseError>
{
    let signature_bin = base64::decode(signature_b64)?;
    if signature_bin.len() != 74 {
        return Err(ParseError::InvalidSignatureLength);
    };

    let mut signature_algorithm = [0; 2];
    let mut _key_id = [0; 8];
    let mut signature = [0; 64];
    signature_algorithm.copy_from_slice(&signature_bin[0..2]);
    _key_id.copy_from_slice(&signature_bin[2..10]);
    signature.copy_from_slice(&signature_bin[10..74]);

    if signature_algorithm.as_ref() != MINISIGN_SIGNATURE_HASHED_CODE {
        return Err(ParseError::InvalidSignatureType);
    };
    Ok(signature)
}

fn _verify_ed25519_signature(
    message: &str,
    signer: [u8; 32],
    signature: [u8; 64],
) -> Result<(), SignatureError> {
    let signature = Signature::from_bytes(&signature)?;
    let public_key = PublicKey::from_bytes(&signer)?;
    let mut hasher = Blake2b512::new();
    hasher.update(message);
    let hash = hasher.finalize();
    public_key.verify(&hash, &signature)?;
    Ok(())
}

#[derive(thiserror::Error, Debug)]
pub enum VerificationError {
    #[error(transparent)]
    InvalidKey(#[from] MulticodecError),

    #[error(transparent)]
    ParseError(#[from] ParseError),

    #[error(transparent)]
    SignatureError(#[from] SignatureError),
}

pub fn verify_minisign_signature(
    signer: &DidKey,
    message: &str,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let ed25519_key = signer.try_ed25519_key()?;
    let ed25519_signature = signature.try_into()
        .map_err(|_| ParseError::InvalidSignatureLength)?;
    let message = format!("{}\n", message);
    _verify_ed25519_signature(
        &message,
        ed25519_key,
        ed25519_signature,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_minisign_signature() {
        let minisign_key =
            "RWSA58rRENpGFYwAjRjbdST7VHFoIuH9JBHfO2u6i5JgANPIoQhABAF/";
        let message = "test";
        let minisign_signature =
            "RUSA58rRENpGFVKxdZGMG1WdIJ+dlyP83qOqw6GP0H/Li6Brug2A3mFKLtleIRLi6IIG0smzOlX5CEsisNnc897OUHIOSNLsQQs=";
        let signer = minisign_key_to_did(minisign_key).unwrap();
        let signature_bin = parse_minisign_signature(minisign_signature).unwrap();
        let result = verify_minisign_signature(&signer, message, &signature_bin);
        assert_eq!(result.is_ok(), true);
    }
}
