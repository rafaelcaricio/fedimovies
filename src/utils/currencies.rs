use crate::ethereum::identity::ETHEREUM_EIP191_PROOF;

pub enum Currency {
    Ethereum,
}

impl Currency {
    fn code(&self) -> String {
        match self {
            Self::Ethereum => "ETH",
        }.to_string()
    }

    /// Network ID and chain ID according to CAIP-2 standard
    /// https://github.com/ChainAgnostic/CAIPs/blob/master/CAIPs/caip-2.md
    pub fn caip2(&self) -> (String, String) {
        let (network_id, chain_id) = match self {
            Self::Ethereum => ("eip155", "1"),
        };
        (network_id.to_string(), chain_id.to_string())
    }

    pub fn normalize_address(&self, address: &str) -> String {
        match self {
            Self::Ethereum => address.to_lowercase(),
        }
    }
}

pub fn get_currency_field_name(currency: &Currency) -> String {
    format!("${}", currency.code())
}

pub fn get_identity_proof_field_name(proof_type: &str) -> Option<String> {
    let field_name = match proof_type {
        ETHEREUM_EIP191_PROOF => "$ETH".to_string(),
        _ => return None,
    };
    Some(field_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_currency_field_name() {
        let ethereum = Currency::Ethereum;
        assert_eq!(
            get_currency_field_name(&ethereum),
            "$ETH",
        );
    }
}
