use crate::errors::ValidationError;
use crate::identity::did_pkh::DidPkh;
use super::signatures::recover_address;
use super::utils::address_to_string;

// Version 00
pub const ETHEREUM_EIP191_PROOF: &str = "ethereum-eip191-00";

/// Verifies proof of address ownership
pub fn verify_eip191_identity_proof(
    did: &DidPkh,
    message: &str,
    signature: &str,
) -> Result<(), ValidationError> {
    let signature_data = signature.parse()
        .map_err(|_| ValidationError("invalid signature string"))?;
    let signer = recover_address(message.as_bytes(), &signature_data)
        .map_err(|_| ValidationError("invalid signature"))?;
    if address_to_string(signer) != did.address.to_lowercase() {
        return Err(ValidationError("invalid proof"));
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use web3::signing::{Key, SecretKeyRef};
    use crate::ethereum::signatures::{
        generate_ecdsa_key,
        sign_message,
    };
    use crate::ethereum::utils::address_to_string;
    use crate::utils::currencies::Currency;
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
