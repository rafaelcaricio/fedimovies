use serde::Serialize;

use crate::errors::ValidationError;
use crate::identity::did_pkh::DidPkh;
use super::signatures::recover_address;
use super::utils::address_to_string;

// Version 00
pub const ETHEREUM_EIP191_PROOF: &str = "ethereum-eip191-00";

// https://www.w3.org/TR/vc-data-model/#credential-subject
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Claim {
    id: String, // actor ID
    owner_of: String, // DID
}

/// Creates address ownership claim and prepares it for signing
pub fn create_identity_claim(
    actor_id: &str,
    did: &DidPkh,
) -> Result<String, serde_json::Error> {
    let claim = Claim {
        id: actor_id.to_string(),
        owner_of: did.to_string(),
    };
    let message = serde_json::to_string(&claim)?;
    Ok(message)
}

/// Verifies proof of address ownership
pub fn verify_eip191_identity_proof(
    actor_id: &str,
    did: &DidPkh,
    signature: &str,
) -> Result<(), ValidationError> {
    let message = create_identity_claim(actor_id, did)
        .map_err(|_| ValidationError("invalid claim"))?;
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
        let actor_id = "https://example.com/users/test";
        let secret_key = generate_ecdsa_key();
        let secret_key_ref = SecretKeyRef::new(&secret_key);
        let secret_key_str = secret_key.display_secret().to_string();
        let address = address_to_string(secret_key_ref.address());
        let did = DidPkh::from_address(&ETHEREUM, &address);
        let message = create_identity_claim(actor_id, &did).unwrap();
        let signature = sign_message(&secret_key_str, message.as_bytes())
            .unwrap().to_string();
        let result = verify_eip191_identity_proof(actor_id, &did, &signature);
        assert_eq!(result.is_ok(), true);
    }
}
