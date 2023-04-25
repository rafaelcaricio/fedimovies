use chrono::{DateTime, Utc};
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use fedimovies_utils::{
    canonicalization::{canonicalize_object, CanonicalizationError},
    crypto_rsa::create_rsa_sha256_signature,
    did_key::DidKey,
    did_pkh::DidPkh,
    multibase::encode_multibase_base58btc,
};

pub(super) const PROOF_KEY: &str = "proof";
pub(super) const PROOF_PURPOSE: &str = "assertionMethod";

/// Data Integrity Proof
/// https://w3c.github.io/vc-data-integrity/
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrityProof {
    #[serde(rename = "type")]
    pub proof_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cryptosuite: Option<String>,
    pub proof_purpose: String,
    pub verification_method: String,
    pub created: DateTime<Utc>,
    pub proof_value: String,
}

#[derive(thiserror::Error, Debug)]
pub enum JsonSignatureError {
    #[error(transparent)]
    JsonError(#[from] serde_json::Error),

    #[error(transparent)]
    CanonicalizationError(#[from] CanonicalizationError),

    #[error("signing error")]
    SigningError(#[from] rsa::errors::Error),

    #[error("invalid object")]
    InvalidObject,
}

pub fn sign_object(
    _object: &Value,
    _signer_key: &RsaPrivateKey,
    _signer_key_id: &str,
) -> Result<Value, JsonSignatureError> {
    Err(JsonSignatureError::InvalidObject)
}

pub fn is_object_signed(object: &Value) -> bool {
    object.get(PROOF_KEY).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fedimovies_utils::crypto_rsa::generate_weak_rsa_key;
    use serde_json::json;

    #[test]
    fn test_sign_object() {
        let signer_key = generate_weak_rsa_key().unwrap();
        let signer_key_id = "https://example.org/users/test#main-key";
        let object = json!({
            "type": "Create",
            "actor": "https://example.org/users/test",
            "id": "https://example.org/objects/1",
            "to": [
                "https://example.org/users/yyy",
                "https://example.org/users/xxx",
            ],
            "object": {
                "type": "Note",
                "content": "test",
            },
        });
        let result = sign_object(&object, &signer_key, signer_key_id).unwrap();

        assert!(is_object_signed(&result));
        assert_eq!(result["actor"], object["actor"]);
        assert_eq!(result["object"], object["object"]);
        let signature_date = result["proof"]["created"].as_str().unwrap();
        // Put * in place of date to avoid escaping all curly brackets
        let expected_result = r#"{"actor":"https://example.org/users/test","id":"https://example.org/objects/1","object":{"content":"test","type":"Note"},"proof":{"created":"*","proofPurpose":"assertionMethod","proofValue":"z2Gh9LYrXjSqFrkia6gMg7xp2wftn1hqmYeEXxrsH9Eh6agB2VYraSYrDoSufbXEHnnyHMCoDSAriLpVacj6E4LFK","type":"MitraJcsRsaSignature2022","verificationMethod":"https://example.org/users/test#main-key"},"to":["https://example.org/users/yyy","https://example.org/users/xxx"],"type":"Create"}"#;
        assert_eq!(
            serde_json::to_string(&result).unwrap(),
            expected_result.replace('*', signature_date),
        );
    }
}
