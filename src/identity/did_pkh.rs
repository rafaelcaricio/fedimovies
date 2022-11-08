/// https://github.com/w3c-ccg/did-pkh/blob/main/did-pkh-method-draft.md
use std::convert::TryInto;
use std::str::FromStr;

use regex::Regex;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::Error as DeserializerError,
};

use crate::utils::caip2::ChainId;
use crate::utils::currencies::Currency;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_did_string_conversion() {
        let address = "0xB9C5714089478a327F09197987f16f9E5d936E8a";
        let ethereum = Currency::Ethereum;
        let did = DidPkh::from_address(&ethereum, address);
        assert_eq!(did.currency().unwrap(), ethereum);
        assert_eq!(did.address, address.to_lowercase());

        let did_str = did.to_string();
        assert_eq!(
            did_str,
            "did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a",
        );

        let did: DidPkh = did_str.parse().unwrap();
        assert_eq!(did.address, address.to_lowercase());
    }
}
