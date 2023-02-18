use crate::identity::did_pkh::DidPkh;
use super::signatures::{recover_address, SignatureError};
use super::utils::address_to_string;

#[derive(thiserror::Error, Debug)]
pub enum Eip191VerificationError {
    #[error(transparent)]
    InvalidSignature(#[from] SignatureError),

    #[error("invalid signer")]
    InvalidSigner,
}

pub fn verify_eip191_signature(
    did: &DidPkh,
    message: &str,
    signature_hex: &str,
) -> Result<(), Eip191VerificationError> {
    let signature_data = signature_hex.parse()?;
    let signer = recover_address(message.as_bytes(), &signature_data)?;
    if address_to_string(signer) != did.address.to_lowercase() {
        return Err(Eip191VerificationError::InvalidSigner);
    };
    Ok(())
}

/// Verifies proof of address ownership
pub fn verify_eip191_identity_proof(
    did: &DidPkh,
    message: &str,
    signature_hex: &str,
) -> Result<(), Eip191VerificationError> {
    verify_eip191_signature(did, message, signature_hex)
}

#[cfg(test)]
mod tests {
    use web3::signing::{Key, SecretKeyRef};
    use mitra_utils::currencies::Currency;
    use crate::ethereum::{
        signatures::{
            generate_ecdsa_key,
            sign_message,
        },
        utils::address_to_string,
    };
    use super::*;

    const ETHEREUM: Currency = Currency::Ethereum;

    #[test]
    fn test_verify_eip191_identity_proof() {
        let message = "test";
        let secret_key = generate_ecdsa_key();
        let secret_key_ref = SecretKeyRef::new(&secret_key);
        let secret_key_str = secret_key.display_secret().to_string();
        let address = address_to_string(secret_key_ref.address());
        let did = DidPkh::from_address(&ETHEREUM, &address);
        let signature = sign_message(&secret_key_str, message.as_bytes())
            .unwrap().to_string();
        let result = verify_eip191_identity_proof(&did, message, &signature);
        assert_eq!(result.is_ok(), true);
    }
}
