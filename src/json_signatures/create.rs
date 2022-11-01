use chrono::{DateTime, Utc};
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::utils::crypto::sign_message;
use super::canonicalization::{canonicalize_object, CanonicalizationError};

/// Data Integrity Proof
/// https://w3c.github.io/vc-data-integrity/
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proof {
    #[serde(rename = "type")]
    pub proof_type: String,
    pub proof_purpose: String,
    pub verification_method: String,
    pub created: DateTime<Utc>,
    pub proof_value: String,
}

pub const PROOF_KEY: &str = "proof";

// Similar to https://identity.foundation/JcsEd25519Signature2020/
// - Canonicalization algorithm: JCS
// - Digest algorithm: SHA-256
// - Signature algorithm: RSASSA-PKCS1-v1_5
pub const PROOF_TYPE: &str = "JcsRsaSignature2022";

pub const PROOF_PURPOSE: &str = "assertionMethod";

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

    #[error("already signed")]
    AlreadySigned,
}

pub fn sign_object(
    object: &Value,
    signer_key: &RsaPrivateKey,
    signer_key_id: &str,
) -> Result<Value, JsonSignatureError> {
    // Canonicalize
    let message = canonicalize_object(object)?;
    // Sign
    let signature_b64 = sign_message(signer_key, &message)?;
    // Insert proof
    let proof = Proof {
        proof_type: PROOF_TYPE.to_string(),
        proof_purpose: PROOF_PURPOSE.to_string(),
        verification_method: signer_key_id.to_string(),
        created: Utc::now(),
        proof_value: signature_b64,
    };
    let proof_value = serde_json::to_value(proof)?;
    let mut object_value = serde_json::to_value(object)?;
    let object_map = object_value.as_object_mut()
        .ok_or(JsonSignatureError::InvalidObject)?;
    if object_map.contains_key(PROOF_KEY) {
        return Err(JsonSignatureError::AlreadySigned);
    };
    object_map.insert(PROOF_KEY.to_string(), proof_value);
    Ok(object_value)
}

pub fn is_object_signed(object: &Value) -> bool {
    object.get(PROOF_KEY).is_some()
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::utils::crypto::generate_weak_private_key;
    use super::*;

    #[test]
    fn test_sign_object() {
        let signer_key = generate_weak_private_key().unwrap();
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
        let expected_result = r#"{"actor":"https://example.org/users/test","id":"https://example.org/objects/1","object":{"content":"test","type":"Note"},"proof":{"created":"*","proofPurpose":"assertionMethod","proofValue":"P4ye1hDvrGQCCClzHfCU9xobMAeqlUfgEWGlZfOTE3WmjH8JC/OJwlsjUMOUwTVlyKStp+AY+zzU4z6mjZN0Ug==","type":"JcsRsaSignature2022","verificationMethod":"https://example.org/users/test#main-key"},"to":["https://example.org/users/yyy","https://example.org/users/xxx"],"type":"Create"}"#;
        assert_eq!(
            serde_json::to_string(&result).unwrap(),
            expected_result.replace('*', signature_date),
        );
    }
}
