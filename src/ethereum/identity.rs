use std::convert::TryInto;
use std::str::FromStr;

use regex::Regex;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::Error as DeserializerError,
};

use crate::errors::ValidationError;
use crate::utils::caip2::ChainId;
use crate::utils::currencies::Currency;
use super::signatures::recover_address;
use super::utils::address_to_string;

// Version 00
pub const ETHEREUM_EIP191_PROOF: &str = "ethereum-eip191-00";

// https://github.com/w3c-ccg/did-pkh/blob/main/did-pkh-method-draft.md
#[derive(Clone, Debug, PartialEq)]
pub struct DidPkh {
    pub chain_id: ChainId,
    pub address: String,
}

impl DidPkh {
    pub fn from_address(currency: &Currency, address: &str) -> Self {
        let chain_id = match currency {
            Currency::Ethereum => ChainId::ethereum_mainnet(),
            Currency::Monero => unimplemented!(),
        };
        let address = currency.normalize_address(address);
        Self { chain_id, address }
    }

    pub fn currency(&self) -> Option<Currency> {
        (&self.chain_id).try_into().ok()
    }
}

impl ToString for DidPkh {
    fn to_string(&self) -> String {
        format!(
            "did:pkh:{}:{}:{}",
            self.chain_id.namespace,
            self.chain_id.reference,
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
            chain_id: ChainId {
                namespace: caps["network"].to_string(),
                reference: caps["chain"].to_string(),
            },
            address: caps["address"].to_string(),
        };
        Ok(did)
    }
}

impl Serialize for DidPkh {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer
    {
        let did_str = self.to_string();
        serializer.serialize_str(&did_str)
    }
}

impl<'de> Deserialize<'de> for DidPkh {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de>
    {
        let did_str: String = Deserialize::deserialize(deserializer)?;
        did_str.parse().map_err(DeserializerError::custom)
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

    const ETHEREUM: Currency = Currency::Ethereum;

    #[test]
    fn test_did_string_conversion() {
        let address = "0xB9C5714089478a327F09197987f16f9E5d936E8a";
        let did = DidPkh::from_address(&ETHEREUM, address);
        assert_eq!(did.currency().unwrap(), ETHEREUM);
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
        let did = DidPkh::from_address(&ETHEREUM, &address);
        let message = create_identity_claim(actor_id, &did).unwrap();
        let signature = sign_message(&secret_key_str, message.as_bytes()).unwrap().to_string();
        let result = verify_identity_proof(actor_id, &did, &signature);
        assert_eq!(result.is_ok(), true);
    }
}
