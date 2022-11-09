/// https://w3c-ccg.github.io/did-method-key/
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use super::did::DidParseError;

const DID_KEY_RE: &str = r"did:key:(?P<key>z[a-km-zA-HJ-NP-Z1-9]+)";

#[derive(Clone, Debug, PartialEq)]
pub struct DidKey {
    pub key: String,
}

impl FromStr for DidKey {
    type Err = DidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let did_key_re = Regex::new(DID_KEY_RE).unwrap();
        let caps = did_key_re.captures(value).ok_or(DidParseError)?;
        let did_key = Self {
            key: caps["key"].to_string(),
        };
        Ok(did_key)
    }
}

impl fmt::Display for DidKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let did_str = format!("did:key:{}", self.key);
        write!(formatter, "{}", did_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_did_key_string_conversion() {
        let did_str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
        let did_key: DidKey = did_str.parse().unwrap();
        assert_eq!(did_key.to_string(), did_str);
    }
}
