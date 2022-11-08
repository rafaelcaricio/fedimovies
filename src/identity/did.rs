/// https://www.w3.org/TR/did-core/
use std::fmt;
use std::str::FromStr;

use regex::Regex;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::Error as DeserializerError,
};

use super::did_pkh::DidPkh;

const DID_RE: &str = r"did:(?P<method>\w+):.+";

#[derive(Clone, Debug, PartialEq)]
pub enum Did {
    Pkh(DidPkh),
}

#[derive(thiserror::Error, Debug)]
#[error("DID parse error")]
pub struct DidParseError;

impl FromStr for Did {
    type Err = DidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let did_re = Regex::new(DID_RE).unwrap();
        let caps = did_re.captures(value).ok_or(DidParseError)?;
        let did = match &caps["method"] {
            "pkh" => {
                let did_pkh = DidPkh::from_str(value)?;
                Self::Pkh(did_pkh)
            },
            _ => return Err(DidParseError),
        };
        Ok(did)
    }
}

impl fmt::Display for Did {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let did_str = match self {
            Self::Pkh(did_pkh) => did_pkh.to_string(),
        };
        write!(formatter, "{}", did_str)
    }
}

impl<'de> Deserialize<'de> for Did {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let did_str: String = Deserialize::deserialize(deserializer)?;
        did_str.parse().map_err(DeserializerError::custom)
    }
}

impl Serialize for Did {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        let did_str = self.to_string();
        serializer.serialize_str(&did_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_did_string_conversion() {
        let did_str = "did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a";
        let did: Did = did_str.parse().unwrap();
        assert!(matches!(did, Did::Pkh(_)));
        assert_eq!(did.to_string(), did_str);
    }
}
