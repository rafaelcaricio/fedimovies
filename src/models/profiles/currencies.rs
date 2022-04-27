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
