use std::str::FromStr;

use rsa::RsaPublicKey;
use serde_json::Value;
use url::Url;

use crate::ethereum::identity::verify_eip191_signature;
use crate::identity::{
    did::Did,
    did_key::DidKey,
    did_pkh::DidPkh,
    minisign::verify_ed25519_signature,
    signatures::SignatureType,
};
use crate::utils::{
    canonicalization::{
        canonicalize_object,
        CanonicalizationError,
    },
    crypto_rsa::verify_rsa_signature,
    multibase::{decode_multibase_base58btc, MultibaseError},
};
use super::create::{
    IntegrityProof,
    PROOF_KEY,
    PROOF_PURPOSE,
};

#[derive(Debug, PartialEq)]
pub enum JsonSigner {
    ActorKeyId(String),
    Did(Did),
}

pub struct SignatureData {
    pub signature_type: SignatureType,
    pub signer: JsonSigner,
    pub message: String,
    pub signature: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
pub enum JsonSignatureVerificationError {
    #[error("invalid object")]
    InvalidObject,

    #[error("no proof")]
    NoProof,

    #[error("{0}")]
    InvalidProof(&'static str),

    #[error(transparent)]
    CanonicalizationError(#[from] CanonicalizationError),

    #[error("invalid encoding")]
    InvalidEncoding(#[from] MultibaseError),

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
    let proof_value = object_map.remove(PROOF_KEY)
        .ok_or(VerificationError::NoProof)?;
    let proof: IntegrityProof = serde_json::from_value(proof_value)
        .map_err(|_| VerificationError::InvalidProof("invalid proof"))?;
    if proof.proof_purpose != PROOF_PURPOSE {
        return Err(VerificationError::InvalidProof("invalid proof purpose"));
    };
    let signature_type = proof.proof_type.parse()
        .map_err(|_| VerificationError::InvalidProof("unsupported proof type"))?;
    let signer = if let Ok(did) = Did::from_str(&proof.verification_method) {
        JsonSigner::Did(did)
    } else if Url::parse(&proof.verification_method).is_ok() {
        JsonSigner::ActorKeyId(proof.verification_method)
    } else {
        return Err(VerificationError::InvalidProof("unsupported verification method"));
    };
    let message = canonicalize_object(&object)?;
    let signature = decode_multibase_base58btc(&proof.proof_value)?;
    let signature_data = SignatureData {
        signature_type,
        signer,
        message,
        signature,
    };
    Ok(signature_data)
}

pub fn verify_rsa_json_signature(
    signature_data: &SignatureData,
    signer_key: &RsaPublicKey,
) -> Result<(), VerificationError> {
    let is_valid_signature = verify_rsa_signature(
        signer_key,
        &signature_data.message,
        &signature_data.signature,
    );
    if !is_valid_signature {
        return Err(VerificationError::InvalidSignature);
    };
    Ok(())
}

pub fn verify_eip191_json_signature(
    signer: &DidPkh,
    message: &str,
    signature: &[u8],
) -> Result<(), VerificationError> {
    let signature_hex = hex::encode(signature);
    verify_eip191_signature(signer, message, &signature_hex)
        .map_err(|_| VerificationError::InvalidSignature)
}

pub fn verify_ed25519_json_signature(
    signer: &DidKey,
    message: &str,
    signature: &[u8],
) -> Result<(), VerificationError> {
    verify_ed25519_signature(signer, message, signature)
        .map_err(|_| VerificationError::InvalidSignature)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::json_signatures::create::sign_object;
    use crate::utils::{
        crypto_rsa::generate_weak_rsa_key,
        currencies::Currency,
    };
    use super::*;

    #[test]
    fn test_get_json_signature_eip191() {
        let signed_object = json!({
            "type": "Test",
            "id": "https://example.org/objects/1",
            "proof": {
                "type": "JcsEip191Signature2022",
                "proofPurpose": "assertionMethod",
                "verificationMethod": "did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a",
                "created": "2020-11-05T19:23:24Z",
                "proofValue": "zE5J",
            },
        });
        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.signature_type,
            SignatureType::JcsEip191Signature,
        );
        let expected_signer = JsonSigner::Did(Did::Pkh(DidPkh::from_address(
            &Currency::Ethereum,
            "0xb9c5714089478a327f09197987f16f9e5d936e8a",
        )));
        assert_eq!(signature_data.signer, expected_signer);
        assert_eq!(hex::encode(signature_data.signature), "abcd");
    }

    #[test]
    fn test_create_and_verify_signature() {
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
        let signed_object = sign_object(
            &object,
            &signer_key,
            signer_key_id,
        ).unwrap();

        let signature_data = get_json_signature(&signed_object).unwrap();
        assert_eq!(
            signature_data.signature_type,
            SignatureType::JcsRsaSignature,
        );
        let expected_signer = JsonSigner::ActorKeyId(signer_key_id.to_string());
        assert_eq!(signature_data.signer, expected_signer);

        let signer_public_key = RsaPublicKey::from(signer_key);
        let result = verify_rsa_json_signature(
            &signature_data,
            &signer_public_key,
        );
        assert_eq!(result.is_ok(), true);
    }
}
