use std::str::FromStr;

use regex::Regex;
use serde::Serialize;

use crate::errors::ValidationError;
use super::signatures::recover_address;
use super::utils::address_to_string;

// Version 00
pub const ETHEREUM_EIP191_PROOF: &str = "ethereum-eip191-00";

// https://github.com/w3c-ccg/did-pkh/blob/main/did-pkh-method-draft.md
pub struct DidPkh {
    network_id: String,
    chain_id: String,
    pub address: String,
}

impl DidPkh {
    pub fn from_ethereum_address(address: &str) -> Self {
        // Ethereum mainnet
        // https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-3.md
        Self {
            network_id: "eip155".to_string(),
            chain_id: "1".to_string(),
            address: address.to_lowercase(),
        }
    }
}

impl ToString for DidPkh {
    fn to_string(&self) -> String {
        format!(
            "did:pkh:{}:{}:{}",
            self.network_id,
            self.chain_id,
            self.address,
        )
    }
}

#[derive(thiserror::Error, Debug)]
#[error("DID parse error")]
pub struct DidParseError;

// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-10.md#syntax
const DID_PKH_RE: &str = r"did:pkh:(?P<network>[-a-z0-9]{3,8}):(?P<chain>[-a-zA-Z0-9]{1,32}):(?P<address>[a-zA-Z0-9]{1,64})";

impl FromStr for DidPkh {
    type Err = DidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let did_pkh_re = Regex::new(DID_PKH_RE).unwrap();
        let caps = did_pkh_re.captures(value).ok_or(DidParseError)?;
        let did = Self {
            network_id: caps["network"].to_string(),
            chain_id: caps["chain"].to_string(),
            address: caps["address"].to_string(),
        };
        Ok(did)
    }
}

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
pub fn verify_identity_proof(
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
    use super::*;

    #[test]
    fn test_did_string_conversion() {
        let address = "0xB9C5714089478a327F09197987f16f9E5d936E8a";
        let did = DidPkh::from_ethereum_address(address);
        assert_eq!(did.address, address.to_lowercase());

        let did_str = did.to_string();
        assert_eq!(
            did_str,
            "did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a",
        );

        let did: DidPkh = did_str.parse().unwrap();
        assert_eq!(did.address, address.to_lowercase());
    }

    #[test]
    fn test_verify_identity_proof() {
        let actor_id = "https://example.com/users/test";
        let secret_key = generate_ecdsa_key();
        let secret_key_ref = SecretKeyRef::new(&secret_key);
        let secret_key_str = secret_key.display_secret().to_string();
        let address = address_to_string(secret_key_ref.address());
        let did = DidPkh::from_ethereum_address(&address);
        let message = create_identity_claim(actor_id, &did).unwrap();
        let signature = sign_message(&secret_key_str, message.as_bytes()).unwrap().to_string();
        let result = verify_identity_proof(actor_id, &did, &signature);
        assert_eq!(result.is_ok(), true);
    }
}
