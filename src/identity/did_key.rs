/// https://w3c-ccg.github.io/did-method-key/
use std::fmt;
use std::str::FromStr;

use regex::Regex;

use super::did::DidParseError;

const DID_KEY_RE: &str = r"did:key:(?P<key>z[a-km-zA-HJ-NP-Z1-9]+)";

#[derive(Clone, Debug, PartialEq)]
pub struct DidKey {
    pub key: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
enum MultibaseError {
    #[error("invalid base string")]
    InvalidBaseString,

    #[error("unknown base")]
    UnknownBase,

    #[error(transparent)]
    DecodeError(#[from] bs58::decode::Error),
}

/// Decodes multibase base58 (bitcoin) value
/// https://github.com/multiformats/multibase
fn decode_multibase_base58btc(value: &str)
    -> Result<Vec<u8>, MultibaseError>
{
    let base = value.chars().next()
        .ok_or(MultibaseError::InvalidBaseString)?;
    // z == base58btc
    // https://github.com/multiformats/multibase#multibase-table
    if base.to_string() != "z" {
        return Err(MultibaseError::UnknownBase);
    };
    let encoded_data = &value[base.len_utf8()..];
    let data = bs58::decode(encoded_data)
        .with_alphabet(bs58::Alphabet::BITCOIN)
        .into_vec()?;
    Ok(data)
}

fn encode_multibase_base58btc(value: &[u8]) -> String {
    let result = bs58::encode(value)
        .with_alphabet(bs58::Alphabet::BITCOIN)
        .into_string();
    format!("z{}", result)
}

impl FromStr for DidKey {
    type Err = DidParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let did_key_re = Regex::new(DID_KEY_RE).unwrap();
        let caps = did_key_re.captures(value).ok_or(DidParseError)?;
        let key = decode_multibase_base58btc(&caps["key"])
            .map_err(|_| DidParseError)?;
        let did_key = Self { key };
        Ok(did_key)
    }
}

impl fmt::Display for DidKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoded_key = encode_multibase_base58btc(&self.key);
        let did_str = format!("did:key:{}", encoded_key);
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
        assert_eq!(did_key.key.len(), 34); // Ed25519 public key
        assert_eq!(did_key.to_string(), did_str);
    }
}
