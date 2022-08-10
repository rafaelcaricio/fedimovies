/// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-2.md
use std::str::FromStr;

use regex::Regex;
use serde::{Deserialize, Deserializer, de::Error as DeserializerError};

const CAIP2_RE: &str = r"(?P<namespace>[-a-z0-9]{3,8}):(?P<reference>[-a-zA-Z0-9]{1,32})";

#[derive(Clone, Debug, PartialEq)]
pub struct ChainId {
    pub namespace: String,
    pub reference: String,
}

#[derive(thiserror::Error, Debug)]
#[error("Chain ID parse error")]
pub struct ChainIdError;

impl FromStr for ChainId {
    type Err = ChainIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let caip2_re = Regex::new(CAIP2_RE).unwrap();
        let caps = caip2_re.captures(value).ok_or(ChainIdError)?;
        let chain_id = Self {
            namespace: caps["namespace"].to_string(),
            reference: caps["reference"].to_string(),
        };
        Ok(chain_id)
    }
}

impl ToString for ChainId {
    fn to_string(&self) -> String {
        format!("{}:{}", self.namespace, self.reference)
    }
}

impl<'de> Deserialize<'de> for ChainId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        String::deserialize(deserializer)?
            .parse().map_err(DeserializerError::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bitcoin_chain_id() {
        let value = "bip122:000000000019d6689c085ae165831e93";
        let chain_id = value.parse::<ChainId>().unwrap();
        assert_eq!(chain_id.namespace, "bip122");
        assert_eq!(chain_id.reference, "000000000019d6689c085ae165831e93");
        assert_eq!(chain_id.to_string(), value);
    }

    #[test]
    fn test_parse_ethereum_chain_id() {
        let value = "eip155:1";
        let chain_id = value.parse::<ChainId>().unwrap();
        assert_eq!(chain_id.namespace, "eip155");
        assert_eq!(chain_id.reference, "1");
        assert_eq!(chain_id.to_string(), value);
    }

    #[test]
    fn test_parse_invalid_chain_id() {
        let value = "eip155/1/abcde";
        assert!(value.parse::<ChainId>().is_err());
    }
}
