use rsa::RsaPublicKey;
use serde_json::Value;

use crate::utils::crypto::verify_signature;
use super::create::{Proof, PROOF_TYPE, PROOF_PURPOSE};

pub struct SignatureData {
    pub key_id: String,
    pub message: String,
    pub signature: String,
}

#[derive(thiserror::Error, Debug)]
pub enum JsonSignatureVerificationError {
    #[error("invalid object")]
    InvalidObject,

    #[error("no proof")]
    NoProof,

    #[error("{0}")]
    InvalidProof(&'static str),

    #[error("canonicalization error")]
    CanonicalizationError,

    #[error("invalid encoding")]
    InvalidEncoding(#[from] base64::DecodeError),

    #[error("invalid signature")]
    InvalidSignature,
}

type VerificationError = JsonSignatureVerificationError;

pub fn get_json_signature(
    object: &Value,
) -> Result<SignatureData, VerificationError> {
    let mut object = object.clone();
    let object_map = object.as_object_mut()
        .ok_or(VerificationError::InvalidObject)?;
    let proof_value = object_map.remove("proof")
        .ok_or(VerificationError::NoProof)?;
    let proof: Proof = serde_json::from_value(proof_value)
        .map_err(|_| VerificationError::InvalidProof("invalid proof"))?;
    if proof.proof_type != PROOF_TYPE ||
        proof.proof_purpose != PROOF_PURPOSE
    {
        return Err(VerificationError::InvalidProof("invalid proof"));
    };
    let canon = serde_jcs::to_string(&object)
        .map_err(|_| VerificationError::CanonicalizationError)?;
    let signature_data = SignatureData {
        key_id: proof.verification_method,
        message: canon,
        signature: proof.proof_value,
    };
    Ok(signature_data)
}

pub fn verify_json_signature(
    signature_data: &SignatureData,
    signer_key: &RsaPublicKey,
) -> Result<(), VerificationError> {
    let is_valid_signature = verify_signature(
        signer_key,
        &signature_data.message,
        &signature_data.signature,
    )?;
    if !is_valid_signature {
        return Err(VerificationError::InvalidSignature);
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::json_signatures::create::sign_object;
    use crate::utils::crypto::generate_weak_private_key;
    use super::*;

    #[test]
    fn test_create_and_verify_signature() {
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
        let signed_object = sign_object(
            &object,
            &signer_key,
            signer_key_id,
        ).unwrap();

        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(signature_data.key_id, signer_key_id);

        let signer_public_key = RsaPublicKey::from(signer_key);
        let result = verify_json_signature(&signature_data, &signer_public_key);
        assert_eq!(result.is_ok(), true);
    }
}
