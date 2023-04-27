/// Proof types
use std::str::FromStr;

use crate::errors::ConversionError;

// Identity proof, version 00
pub const PROOF_TYPE_ID_EIP191: &str = "ethereum-eip191-00";

// Identity proof, version 2022A
pub const PROOF_TYPE_ID_MINISIGN: &str = "MitraMinisignSignature2022A";

// Similar to https://identity.foundation/JcsEd25519Signature2020/
// - Canonicalization algorithm: JCS
// - Digest algorithm: SHA-256
// - Signature algorithm: RSASSA-PKCS1-v1_5
pub const PROOF_TYPE_JCS_RSA: &str = "MitraJcsRsaSignature2022";

// https://w3c.github.io/vc-data-integrity/#dataintegrityproof
pub const DATA_INTEGRITY_PROOF: &str = "DataIntegrityProof";

// Similar to EthereumPersonalSignature2021 but with JCS
pub const PROOF_TYPE_JCS_EIP191_LEGACY: &str = "JcsEip191Signature2022";

#[derive(Debug, PartialEq)]
pub enum ProofType {
    JcsEip191Signature,
    JcsRsaSignature,
}

impl FromStr for ProofType {
    type Err = ConversionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let proof_type = match value {
            PROOF_TYPE_JCS_EIP191_LEGACY => Self::JcsEip191Signature,
            _ => return Err(ConversionError),
        };
        Ok(proof_type)
    }
}

impl ProofType {
    pub fn from_cryptosuite(_value: &str) -> Result<Self, ConversionError> {
        Err(ConversionError)
    }
}
