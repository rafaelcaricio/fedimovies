/// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-2.md
use std::fmt;
use std::str::FromStr;

use regex::Regex;
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
    Serializer,
    de::Error as DeserializerError,
};

use super::currencies::Currency;

const CAIP2_RE: &str = r"(?P<namespace>[-a-z0-9]{3,8}):(?P<reference>[-a-zA-Z0-9]{1,32})";
const CAIP2_ETHEREUM_NAMESPACE: &str = "eip155";
const CAIP2_MONERO_NAMESPACE: &str = "monero"; // unregistered namespace
const ETHEREUM_MAINNET_ID: i32 = 1;
const ETHEREUM_DEVNET_ID: i32 = 31337;

#[derive(Clone, Debug, PartialEq)]
pub struct ChainId {
    pub namespace: String,
    pub reference: String,
}

impl ChainId {
    pub fn ethereum_mainnet() -> Self {
        Self {
            namespace: CAIP2_ETHEREUM_NAMESPACE.to_string(),
            reference: ETHEREUM_MAINNET_ID.to_string(),
        }
    }

    pub fn ethereum_devnet() -> Self {
        Self {
            namespace: CAIP2_ETHEREUM_NAMESPACE.to_string(),
            reference: ETHEREUM_DEVNET_ID.to_string(),
        }
    }

    pub fn is_ethereum(&self) -> bool {
        self.namespace == CAIP2_ETHEREUM_NAMESPACE
    }

    pub fn is_monero(&self) -> bool {
        self.namespace == CAIP2_MONERO_NAMESPACE
    }

    pub fn ethereum_chain_id(&self) -> Result<u32, ChainIdError> {
        if !self.is_ethereum() {
            return Err(ChainIdError("namespace is not eip155"));
        };
        let chain_id: u32 = self.reference.parse()
            .map_err(|_| ChainIdError("invalid EIP-155 chain ID"))?;
        Ok(chain_id)
    }

    pub fn currency(&self) -> Option<Currency> {
        let currency = match self.namespace.as_str() {
            CAIP2_ETHEREUM_NAMESPACE => Currency::Ethereum,
            CAIP2_MONERO_NAMESPACE => Currency::Monero,
            _ => return None,
        };
        Some(currency)
    }
}

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct ChainIdError(&'static str);

impl FromStr for ChainId {
    type Err = ChainIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let caip2_re = Regex::new(CAIP2_RE).unwrap();
        let caps = caip2_re.captures(value)
            .ok_or(ChainIdError("invalid chain ID"))?;
        let chain_id = Self {
            namespace: caps["namespace"].to_string(),
            reference: caps["reference"].to_string(),
        };
        Ok(chain_id)
    }
}

impl fmt::Display for ChainId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.namespace, self.reference)
    }
}

impl Serialize for ChainId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        serializer.serialize_str(&self.to_string())
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

mod sql {
    use postgres_protocol::types::{text_from_sql, text_to_sql};
    use postgres_types::{
        accepts,
        private::BytesMut,
        to_sql_checked,
        FromSql,
        IsNull,
        ToSql,
        Type,
    };
    use super::ChainId;

    impl<'a> FromSql<'a> for ChainId {
        fn from_sql(
            _: &Type,
            raw: &'a [u8],
        ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            let value_str = text_from_sql(raw)?;
            let value: Self = value_str.parse()?;
            Ok(value)
        }

        accepts!(VARCHAR);
    }

    impl ToSql for ChainId {
        fn to_sql(
            &self,
            _: &Type,
            out: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
            let value_str = self.to_string();
            text_to_sql(&value_str, out);
            Ok(IsNull::No)
        }

        accepts!(VARCHAR, TEXT);
        to_sql_checked!();
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

    #[test]
    fn test_ethereum_chain_id() {
        let chain_id: ChainId = "eip155:1".parse().unwrap();
        let result = chain_id.ethereum_chain_id().unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_ethereum_chain_id_not_ethereum() {
        let chain_id: ChainId = "bip122:000000000019d6689c085ae165831e93".parse().unwrap();
        let error = chain_id.ethereum_chain_id().err().unwrap();
        assert!(matches!(error, ChainIdError("namespace is not eip155")));
    }

    #[test]
    fn test_chain_id_conversion() {
        let ethereum_chain_id = ChainId::ethereum_mainnet();
        let currency = ethereum_chain_id.currency().unwrap();
        assert_eq!(currency, Currency::Ethereum);

        let monero_chain_id = ChainId {
            namespace: "monero".to_string(),
            reference: "mainnet".to_string(),
        };
        let currency = monero_chain_id.currency().unwrap();
        assert_eq!(currency, Currency::Monero);
    }
}
