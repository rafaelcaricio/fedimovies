use rsa::RsaPublicKey;
use serde_json::Value;

use crate::ethereum::{
    signatures::recover_address,
    utils::address_to_string,
};
use crate::identity::did_pkh::DidPkh;
use crate::utils::crypto::verify_signature;
use super::canonicalization::{canonicalize_object, CanonicalizationError};
use super::create::{
    IntegrityProof,
    PROOF_TYPE_JCS_EIP191,
    PROOF_TYPE_JCS_RSA,
    PROOF_KEY,
    PROOF_PURPOSE,
};

#[derive(Debug, PartialEq)]
pub enum JsonSigner {
    ActorKeyId(String),
    DidPkh(DidPkh),
}

pub struct SignatureData {
    pub signer: JsonSigner,
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

    #[error(transparent)]
    CanonicalizationError(#[from] CanonicalizationError),

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
    let proof_value = object_map.remove(PROOF_KEY)
        .ok_or(VerificationError::NoProof)?;
    let proof: IntegrityProof = serde_json::from_value(proof_value)
        .map_err(|_| VerificationError::InvalidProof("invalid proof"))?;
    if proof.proof_purpose != PROOF_PURPOSE {
        return Err(VerificationError::InvalidProof("invalid proof purpose"));
    };
    let signer = match proof.proof_type.as_str() {
        PROOF_TYPE_JCS_EIP191 => {
            let did = proof.verification_method.parse()
                .map_err(|_| VerificationError::InvalidProof("invalid DID"))?;
            JsonSigner::DidPkh(did)
        },
        PROOF_TYPE_JCS_RSA => {
            JsonSigner::ActorKeyId(proof.verification_method)
        },
        _ => {
            return Err(VerificationError::InvalidProof("unsupported proof type"));
        },
    };
    let message = canonicalize_object(&object)?;
    let signature_data = SignatureData {
        signer: signer,
        message: message,
        signature: proof.proof_value,
    };
    Ok(signature_data)
}

pub fn verify_jcs_rsa_signature(
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

pub fn verify_jcs_eip191_signature(
    signer: &DidPkh,
    message: &str,
    signature: &str,
) -> Result<(), VerificationError> {
    let signature_data = signature.parse()
        .map_err(|_| VerificationError::InvalidProof("invalid proof"))?;
    let signer_address = recover_address(message.as_bytes(), &signature_data)
        .map_err(|_| VerificationError::InvalidSignature)?;
    if address_to_string(signer_address) != signer.address.to_lowercase() {
        return Err(VerificationError::InvalidSignature);
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use crate::json_signatures::create::sign_object;
    use crate::utils::crypto::generate_weak_private_key;
    use crate::utils::currencies::Currency;
    use super::*;

    #[test]
    fn test_get_json_signature_eip155() {
        let signed_object = json!({
            "type": "Test",
            "id": "https://example.org/objects/1",
            "proof": {
                "type": "JcsEip191Signature2022",
                "proofPurpose": "assertionMethod",
                "verificationMethod": "did:pkh:eip155:1:0xb9c5714089478a327f09197987f16f9e5d936e8a",
                "created": "2020-11-05T19:23:24Z",
                "proofValue": "xxx",
            },
        });
        let signature_data = get_json_signature(&signed_object).unwrap();
        let expected_signer = JsonSigner::DidPkh(DidPkh::from_address(
            &Currency::Ethereum,
            "0xb9c5714089478a327f09197987f16f9e5d936e8a",
        ));
        assert_eq!(signature_data.signer, expected_signer);
        assert_eq!(signature_data.signature, "xxx");
    }

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
        let expected_signer = JsonSigner::ActorKeyId(signer_key_id.to_string());
        assert_eq!(signature_data.signer, expected_signer);

        let signer_public_key = RsaPublicKey::from(signer_key);
        let result = verify_jcs_rsa_signature(
            &signature_data,
            &signer_public_key,
        );
        assert_eq!(result.is_ok(), true);
    }
}
