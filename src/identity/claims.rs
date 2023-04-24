use serde::Serialize;

use mitra_utils::{
    canonicalization::{canonicalize_object, CanonicalizationError},
    did::Did,
};

// https://www.w3.org/TR/vc-data-model/#credential-subject
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Claim {
    id: String, // actor ID
    owner_of: Did,
}

/// Creates key ownership claim and prepares it for signing
pub fn create_identity_claim(actor_id: &str, did: &Did) -> Result<String, CanonicalizationError> {
    let claim = Claim {
        id: actor_id.to_string(),
        owner_of: did.clone(),
    };
    let message = canonicalize_object(&claim)?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitra_utils::{currencies::Currency, did_pkh::DidPkh};

    #[test]
    fn test_create_identity_claim() {
        let actor_id = "https://example.org/users/test";
        let ethereum_address = "0xB9C5714089478a327F09197987f16f9E5d936E8a";
        let did = Did::Pkh(DidPkh::from_address(&Currency::Ethereum, ethereum_address));
        let claim = create_identity_claim(actor_id, &did).unwrap();
        assert_eq!(
            claim,
            r#"{"id":"https://example.org/users/test","ownerOf":"did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a"}"#,
        );
    }
}
