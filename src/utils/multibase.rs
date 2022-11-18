#[derive(thiserror::Error, Debug)]
pub enum MultibaseError {
    #[error("invalid base string")]
    InvalidBaseString,

    #[error("unknown base")]
    UnknownBase,

    #[error(transparent)]
    DecodeError(#[from] bs58::decode::Error),
}

/// Decodes multibase base58 (bitcoin) value
/// https://github.com/multiformats/multibase
pub fn decode_multibase_base58btc(value: &str)
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

pub fn encode_multibase_base58btc(value: &[u8]) -> String {
    let result = bs58::encode(value)
        .with_alphabet(bs58::Alphabet::BITCOIN)
        .into_string();
    format!("z{}", result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base58btc_encode_decode() {
        let value = [1; 20];
        let encoded = encode_multibase_base58btc(&value);
        let decoded = decode_multibase_base58btc(&encoded).unwrap();
        assert_eq!(decoded, value);
    }
}
