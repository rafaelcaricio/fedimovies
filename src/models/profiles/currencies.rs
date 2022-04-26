use crate::ethereum::identity::ETHEREUM_EIP191_PROOF;

pub fn get_currency_field_name(currency_code: &str) -> String {
    format!("${}", currency_code.to_uppercase())
}

pub fn get_identity_proof_field_name(proof_type: &str) -> Option<String> {
    let field_name = match proof_type {
        ETHEREUM_EIP191_PROOF => "$ETH".to_string(),
        _ => return None,
    };
    Some(field_name)
}
